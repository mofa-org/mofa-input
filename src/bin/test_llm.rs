use std::io::{self, Write};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // Find model
    let model_path = PathBuf::from("/Users/yao/Desktop/code/work/mofa-org/mofa-input/models/qwen3-0.6b-q4_k_m.gguf");

    if !model_path.exists() {
        println!("Model not found: {:?}", model_path);
        println!("Please download it first:");
        println!("curl -L -o models/qwen3-0.6b-q4_k_m.gguf \"https://huggingface.co/lmstudio-community/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf\"");
        return Ok(());
    }

    println!("Loading model from {:?}...", model_path);
    let chat = mofa_input::llm::ChatSession::new(&model_path)?;
    println!("Model loaded! Ready for chat.\n");

    loop {
        print!("You: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input == "quit" || input == "exit" {
            break;
        }

        if input == "/clear" {
            chat.clear();
            println!("[History cleared]\n");
            continue;
        }

        if input == "/tokens" {
            println!("[Tokens in cache: {}]\n", chat.token_count());
            continue;
        }

        print!("AI: ");
        io::stdout().flush()?;

        chat.send_stream(input, 512, 0.7, |token| {
            print!("{}", token);
            io::stdout().flush().unwrap();
        });

        println!("\n");
    }

    Ok(())
}
