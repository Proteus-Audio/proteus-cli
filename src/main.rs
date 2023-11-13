use clap::{Arg, ArgMatches};
use symphonia::core::errors::Result;
use log::error;

mod constants;
mod player;
mod prot;
mod buffer;
mod track;

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
        .arg(Arg::new("debug").short('d').help("Show debug output"))
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

fn format_time(time: u32) -> String {
    // Seconds rounded up
    let seconds = (time as f32 / 100.0).ceil() as u32;
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    let hours = minutes / 60;
    let minutes = minutes % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn run(args: &ArgMatches) -> Result<i32> {
    let file_path = args.get_one::<String>("INPUT").unwrap();
    
    println!("file_path: {:?}", file_path);

    // If file is not a .mka file, return an error
    if !(file_path.ends_with(".prot") || file_path.ends_with(".mka")) {
        panic!("File is not a .prot file");
    }

    let thread_start_time = std::time::Instant::now();
    // let sink_mutex = output::play(file_path);
    let player = player::Player::new(file_path);
    println!("Hey there!!");

    player.play();

    // Start sink
    let mut sink_started = false;
    // let sink = sink_mutex.lock().unwrap();
    // sink.play();
    // drop(sink);

    loop {
        // let sink = sink_mutex.lock().unwrap();
        // println!("Sink Len: {:?}", sink.len());
        // if !sink_started && sink.len() > 0 {
        //     sink.play();
        //     sink_started = true;
        //     // sink.sleep_until_end();
        // }
        // let elapsed = thread_start_time.elapsed();
        // let seconds = elapsed.as_secs();
        // let nanos = elapsed.subsec_nanos();
        // let time = seconds as f64 + nanos as f64 / 1_000_000_000.0;
        // let time_str = format!("{:.2}", time);
        // let time_str = time_str.as_str();
        // let time_str = time_str.trim_end_matches("0");
        // let time_str = time_str.trim_end_matches(".");
        println!("Time: {}", format_time(player.get_time()));

        // drop(sink);
        println!("Playing: {}", player.is_playing());
        
        if thread_start_time.elapsed().as_secs() > 10 && player.is_playing() {
            player.pause();
            // break;
        }

        if thread_start_time.elapsed().as_secs() > 12 && !player.is_playing() {
            player.play();
            // break;
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(0)
}
