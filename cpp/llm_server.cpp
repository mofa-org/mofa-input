#include "llm_server.h"
#include "llama.cpp/include/llama.h"
#include <cstring>
#include <string>
#include <vector>
#include <thread>
#include <iostream>

struct LlmContext {
    llama_model* model = nullptr;
    llama_context* ctx = nullptr;
    std::vector<llama_chat_message> chat_history;

    ~LlmContext() {
        if (ctx) llama_free(ctx);
        if (model) llama_free_model(model);
    }
};

// Helper: sample token
static llama_token sample_token(llama_context* ctx, llama_sampler* smpl) {
    return llama_sampler_sample(smpl, ctx, -1);
}

extern "C" {

LlmContext* llm_init(const char* model_path) {
    llama_backend_init();

    auto* llm = new LlmContext();

    // Model params - enable Metal GPU
    llama_model_params model_params = llama_model_default_params();
    model_params.n_gpu_layers = 100;  // Offload all to GPU

    llm->model = llama_load_model_from_file(model_path, model_params);
    if (!llm->model) {
        std::cerr << "Failed to load model from: " << model_path << std::endl;
        delete llm;
        return nullptr;
    }

    // Context params
    llama_context_params ctx_params = llama_context_default_params();
    ctx_params.n_ctx = 8192;
    ctx_params.n_batch = 2048;
    ctx_params.n_threads = std::thread::hardware_concurrency() / 2;

    llm->ctx = llama_new_context_with_model(llm->model, ctx_params);
    if (!llm->ctx) {
        std::cerr << "Failed to create context" << std::endl;
        delete llm;
        return nullptr;
    }

    return llm;
}

void llm_free(LlmContext* llm) {
    delete llm;
}

void llm_kv_clear(LlmContext* llm) {
    if (llm && llm->ctx) {
        llama_kv_cache_clear(llm->ctx);
    }
}

int llm_kv_count(LlmContext* llm) {
    if (llm && llm->ctx) {
        return llama_get_kv_cache_token_count(llm->ctx);
    }
    return 0;
}

void llm_chat_clear(LlmContext* llm) {
    llm->chat_history.clear();
    llm_kv_clear(llm);
}

void llm_chat_add_user(LlmContext* llm, const char* message) {
    llm->chat_history.push_back({"user", strdup(message)});
}

static void add_to_batch(llama_batch& batch, llama_token token, llama_pos pos, bool logits) {
    batch.token[batch.n_tokens] = token;
    batch.pos[batch.n_tokens] = pos;
    batch.n_seq_id[batch.n_tokens] = 1;
    batch.seq_id[batch.n_tokens][0] = 0;
    batch.logits[batch.n_tokens] = logits ? 1 : 0;
    batch.n_tokens++;
}

static char* generate_response(LlmContext* llm, int32_t max_tokens, float temperature,
                                TokenCallback callback, void* user_data) {
    std::string response;

    // Apply chat template to get prompt
    std::vector<char> buf(8192);
    int32_t len = llama_chat_apply_template(
        llm->model,
        nullptr,  // use default template
        llm->chat_history.data(),
        llm->chat_history.size(),
        true,  // add assistant prompt
        buf.data(),
        buf.size()
    );

    if (len < 0) {
        return strdup("[Error: chat template failed]");
    }

    if (len > (int32_t)buf.size()) {
        buf.resize(len + 1);
        llama_chat_apply_template(
            llm->model, nullptr,
            llm->chat_history.data(), llm->chat_history.size(),
            true, buf.data(), buf.size()
        );
    }

    // Ensure buffer is null-terminated at the correct position
    buf[len] = '\0';

    // Tokenize - IMPORTANT: parse_special=true to handle <|im_start|>, <|im_end|> etc.
    // First try with estimated buffer size
    int n_tokens_est = len + 16;  // prompt length + some extra for special tokens
    std::vector<llama_token> tokens(n_tokens_est);

    int32_t n_tokens = llama_tokenize(
        llm->model, buf.data(), len,
        tokens.data(), tokens.size(), true, true  // add_special=true, parse_special=true
    );

    if (n_tokens < 0) {
        // Buffer too small, resize and retry
        n_tokens = -n_tokens;
        tokens.resize(n_tokens);
        int32_t check = llama_tokenize(
            llm->model, buf.data(), len,
            tokens.data(), tokens.size(), true, true
        );
        if (check != n_tokens) {
            return strdup("[Error: tokenization failed]");
        }
    } else {
        tokens.resize(n_tokens);
    }

    // Decode prompt
    llama_batch batch = llama_batch_init(tokens.size(), 0, 1);
    for (size_t i = 0; i < tokens.size(); i++) {
        add_to_batch(batch, tokens[i], (llama_pos)i, i == tokens.size() - 1);
    }
    int32_t decode_result = llama_decode(llm->ctx, batch);
    llama_batch_free(batch);

    if (decode_result != 0) {
        return strdup("[Error: decode failed]");
    }

    // Create sampler
    llama_sampler* smpl = llama_sampler_chain_init(llama_sampler_chain_default_params());
    llama_sampler_chain_add(smpl, llama_sampler_init_temp(temperature));
    llama_sampler_chain_add(smpl, llama_sampler_init_dist(12345));

    // Generate
    int32_t n_pos = tokens.size();
    for (int32_t i = 0; i < max_tokens && n_pos < 8192; i++) {
        llama_token new_token = sample_token(llm->ctx, smpl);

        if (llama_token_is_eog(llm->model, new_token)) {
            break;
        }

        char piece[256];
        int32_t n = llama_token_to_piece(llm->model, new_token, piece, sizeof(piece), 0, true);
        if (n > 0) {
            response.append(piece, n);
            if (callback) {
                piece[n] = '\0';
                callback(piece, user_data);
            }
        }

        llama_batch batch_next = llama_batch_get_one(&new_token, 1);
        llama_decode(llm->ctx, batch_next);
        n_pos++;
    }

    llama_sampler_free(smpl);

    // Add assistant response to history
    llm->chat_history.push_back({"assistant", strdup(response.c_str())});

    return strdup(response.c_str());
}

char* llm_chat_respond(LlmContext* llm, int32_t max_tokens, float temperature) {
    return generate_response(llm, max_tokens, temperature, nullptr, nullptr);
}

void llm_chat_respond_stream(LlmContext* llm, int32_t max_tokens, float temperature,
                              TokenCallback callback, void* user_data) {
    char* result = generate_response(llm, max_tokens, temperature, callback, user_data);
    llm_free_string(result);
}

void llm_free_string(char* str) {
    free(str);
}

// Legacy API
char* llm_generate(LlmContext* llm, const char* prompt, int32_t max_tokens, float temperature) {
    llm_chat_clear(llm);
    llm_chat_add_user(llm, prompt);
    return llm_chat_respond(llm, max_tokens, temperature);
}

void llm_generate_stream(LlmContext* llm, const char* prompt, int32_t max_tokens, float temperature,
                         TokenCallback callback, void* user_data) {
    llm_chat_clear(llm);
    llm_chat_add_user(llm, prompt);
    llm_chat_respond_stream(llm, max_tokens, temperature, callback, user_data);
}

} // extern "C"
