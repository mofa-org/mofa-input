#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle types
typedef struct LlmContext LlmContext;
typedef struct LlmSampler LlmSampler;

// Callback for streaming
typedef void (*TokenCallback)(const char* token, void* user_data);

// ===== Core API =====

// Initialize LLM from GGUF file
LlmContext* llm_init(const char* model_path);

// Free LLM context
void llm_free(LlmContext* ctx);

// ===== Generation API =====

// Generate text (blocking, returns allocated string)
char* llm_generate(LlmContext* ctx, const char* prompt, int max_tokens, float temperature);

// Generate with streaming callback
void llm_generate_stream(LlmContext* ctx, const char* prompt, int max_tokens, float temperature,
                         TokenCallback callback, void* user_data);

// Free string returned by llm_generate
void llm_free_string(char* str);

// ===== KV Cache API =====

// Clear KV cache (start new conversation)
void llm_kv_clear(LlmContext* ctx);

// Get number of tokens in cache
int llm_kv_count(LlmContext* ctx);

// ===== Multi-turn Conversation API =====

// Add user message to history (does not generate)
void llm_chat_add_user(LlmContext* ctx, const char* message);

// Generate assistant response based on history
char* llm_chat_respond(LlmContext* ctx, int max_tokens, float temperature);

// Stream assistant response
void llm_chat_respond_stream(LlmContext* ctx, int max_tokens, float temperature,
                              TokenCallback callback, void* user_data);

// Clear conversation history
void llm_chat_clear(LlmContext* ctx);

#ifdef __cplusplus
}
#endif
