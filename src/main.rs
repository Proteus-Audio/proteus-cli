use clap::{Arg, ArgMatches};
use symphonia::core::errors::Result;
use log::error;

mod constants;
mod output;
mod prot;
mod buffer;

fn main() {
    let args = clap::Command::new("Prot Play")
        .version("1.0")
        .author("Adam Howard <adam.thomas.howard@gmail.com>")
        .about("Play Prot audio")
        .arg(
            Arg::new("seek")
                .long("seek")
                .short('s')
                .value_name("TIME")
                .help("Seek to the given time in seconds")
                .conflicts_with_all(&["verify", "decode-only", "verify-only", "probe-only"]),
        )
        .arg(
            Arg::new("track").long("track").short('t').value_name("TRACK").help("The track to use"),
        )
        .arg(
            Arg::new("decode-only")
                .long("decode-only")
                .help("Decode, but do not play the audio")
                .conflicts_with_all(&["probe-only", "verify-only", "verify"]),
        )
        .arg(
            Arg::new("probe-only")
                .long("probe-only")
                .help("Only probe the input for metadata")
                .conflicts_with_all(&["decode-only", "verify-only"]),
        )
        .arg(
            Arg::new("verify-only")
                .long("verify-only")
                .help("Verify the decoded audio is valid, but do not play the audio")
                .conflicts_with_all(&["verify"]),
        )
        .arg(
            Arg::new("verify")
                .long("verify")
                .short('v')
                .help("Verify the decoded audio is valid during playback"),
        )
        .arg(Arg::new("no-progress").long("no-progress").help("Do not display playback progress"))
        .arg(
            Arg::new("no-gapless").long("no-gapless").help("Disable gapless decoding and playback"),
        )
        .arg(
            Arg::new("INPUT")
                .help("The input file path, or - to use standard input")
                .required(true)
                .index(1),
        )
        .get_matches();


    // For any error, return an exit code -1. Otherwise return the exit code provided.
    let code = match run(&args) {
        Ok(code) => code,
        Err(err) => {
            error!("{}", err.to_string().to_lowercase());
            -1
        }
    };

    std::process::exit(code)
}

fn run(args: &ArgMatches) -> Result<i32> {
    let file_path = args.get_one::<String>("INPUT").unwrap();
    
    println!("file_path: {:?}", file_path);

    // If file is not a .mka file, return an error
    if !file_path.ends_with(".mka") {
        panic!("File is not a .mka file");
    }

    output::play(file_path);

    Ok(0)
}
