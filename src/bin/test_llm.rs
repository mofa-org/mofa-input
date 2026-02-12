use std::io::{self, Write};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // Parse command line arguments for model selection
    let args: Vec<String> = std::env::args().collect();

    // Default to 0.5B model, allow "1.5b" or "7b" as arguments
    let model_path = if args.len() > 1 && args[1] == "1.5b" {
        PathBuf::from("/Users/yao/Desktop/code/work/mofa-org/mofa-input/models/qwen2.5-1.5b-q4_k_m.gguf")
    } else if args.len() > 1 && args[1] == "7b" {
        PathBuf::from("/Users/yao/Desktop/code/work/mofa-org/mofa-input/models/qwen2.5-7b-q4_k_m.gguf")
    } else {
        PathBuf::from("/Users/yao/Desktop/code/work/mofa-org/mofa-input/models/qwen2.5-0.5b-q4_k_m.gguf")
    };

    if !model_path.exists() {
        println!("Model not found: {:?}", model_path);
        println!("Please download it first.");
        return Ok(());
    }

    println!("Loading model from {:?}...", model_path);
    let start = std::time::Instant::now();
    let chat = mofa_input::llm::ChatSession::new(&model_path)?;
    println!("Model loaded in {:?}! Ready for chat.\n", start.elapsed());

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

        let gen_start = std::time::Instant::now();
        chat.send_stream(input, 512, 0.7, |token| {
            print!("{}", token);
            io::stdout().flush().unwrap();
        });
        let elapsed = gen_start.elapsed();
        println!("\n[Generated in {:?}]\n", elapsed);
    }

    Ok(())
}
