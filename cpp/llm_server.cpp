#include "llm_server.h"
#include "llama.cpp/include/llama.h"
#include <cstring>
#include <string>
#include <vector>
#include <thread>

struct LlmContext {
    llama_model* model = nullptr;
    llama_context* ctx = nullptr;
    llama_vocab* vocab = nullptr;
    std::vector<llama_chat_message> chat_history;
    std::vector<char> chat_buffer;

    ~LlmContext() {
        if (ctx) llama_free(ctx);
        if (model) llama_model_free(model);
    }
};

// Helper: sample token
static llama_token sample_token(llama_context* ctx, llama_vocab* vocab, float temperature) {
    llama_sampler* smpl = llama_sampler_chain_init(llama_sampler_chain_default_params());
    llama_sampler_chain_add(smpl, llama_sampler_init_temp(temperature));
    llama_sampler_chain_add(smpl, llama_sampler_init_dist(12345));

    llama_token new_token_id = llama_sampler_sample(smpl, ctx, -1);
    llama_sampler_free(smpl);
    return new_token_id;
}

extern "C" {

LlmContext* llm_init(const char* model_path) {
    llama_backend_init();

    auto* llm = new LlmContext();

    // Model params - enable Metal GPU
    llama_model_params model_params = llama_model_default_params();
    model_params.n_gpu_layers = 100;  // Offload all to GPU

    llm->model = llama_model_load_from_file(model_path, model_params);
    if (!llm->model) {
        delete llm;
        return nullptr;
    }

    llm->vocab = llama_model_get_vocab(llm->model);

    // Context params
    llama_context_params ctx_params = llama_context_default_params();
    ctx_params.n_ctx = 8192;
    ctx_params.n_batch = 2048;
    ctx_params.n_threads = std::thread::hardware_concurrency() / 2;

    llm->ctx = llama_init_from_model(llm->model, ctx_params);
    if (!llm->ctx) {
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
        llama_kv_self_clear(llm->ctx);
    }
}

int llm_kv_count(LlmContext* llm) {
    if (llm && llm->ctx) {
        return llama_kv_self_n_tokens(llm->ctx);
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

static char* generate_response(LlmContext* llm, int max_tokens, float temperature,
                                TokenCallback callback, void* user_data) {
    std::string response;

    // Apply chat template to get prompt
    std::vector<char> buf(8192);
    int len = llama_chat_apply_template(
        llm->vocab,
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

    if (len > (int)buf.size()) {
        buf.resize(len + 1);
        llama_chat_apply_template(
            llm->vocab, nullptr,
            llm->chat_history.data(), llm->chat_history.size(),
            true, buf.data(), buf.size()
        );
    }

    // Tokenize
    std::vector<llama_token> tokens;
    tokens.resize(strlen(buf.data()) + 16);
    int n_tokens = llama_tokenize(
        llm->vocab, buf.data(), strlen(buf.data()),
        tokens.data(), tokens.size(), true, false
    );

    if (n_tokens < 0) {
        return strdup("[Error: tokenization failed]");
    }
    tokens.resize(n_tokens);

    // Decode prompt
    llama_batch batch = llama_batch_init(tokens.size(), 0, 1);
    for (size_t i = 0; i < tokens.size(); i++) {
        llama_batch_add(batch, tokens[i], i, {0}, false);
    }
    batch.logits[batch.n_tokens - 1] = 1;
    llama_decode(llm->ctx, batch);
    llama_batch_free(batch);

    // Generate
    int n_pos = tokens.size();
    for (int i = 0; i < max_tokens && n_pos < 8192; i++) {
        llama_token new_token = sample_token(llm->ctx, llm->vocab, temperature);

        if (llama_vocab_is_eog(llm->vocab, new_token)) {
            break;
        }

        char piece[256];
        int n = llama_token_to_piece(llm->vocab, new_token, piece, sizeof(piece), 0, true);
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

    return strdup(response.c_str());
}

char* llm_chat_respond(LlmContext* llm, int max_tokens, float temperature) {
    return generate_response(llm, max_tokens, temperature, nullptr, nullptr);
}

void llm_chat_respond_stream(LlmContext* llm, int max_tokens, float temperature,
                              TokenCallback callback, void* user_data) {
    char* result = generate_response(llm, max_tokens, temperature, callback, user_data);
    llm_free_string(result);
}

void llm_free_string(char* str) {
    free(str);
}

// Legacy API
char* llm_generate(LlmContext* llm, const char* prompt, int max_tokens, float temperature) {
    llm_chat_clear(llm);
    llm_chat_add_user(llm, prompt);
    return llm_chat_respond(llm, max_tokens, temperature);
}

void llm_generate_stream(LlmContext* llm, const char* prompt, int max_tokens, float temperature,
                         TokenCallback callback, void* user_data) {
    llm_chat_clear(llm);
    llm_chat_add_user(llm, prompt);
    llm_chat_respond_stream(llm, max_tokens, temperature, callback, user_data);
}

} // extern "C"
