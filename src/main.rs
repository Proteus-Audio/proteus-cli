use clap::{Arg, ArgMatches};
use log::error;
use proteus_audio::{info, player, prot};
use symphonia::core::errors::Result;
use rand::Rng;

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
            Arg::new("track")
                .long("track")
                .short('t')
                .value_name("TRACK")
                .help("The track to use"),
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
        .arg(
            Arg::new("no-progress")
                .long("no-progress")
                .help("Do not display playback progress"),
        )
        .arg(
            Arg::new("no-gapless")
                .long("no-gapless")
                .help("Disable gapless decoding and playback"),
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

fn format_time(time: f64) -> String {
    // Seconds rounded up
    let seconds = (time / 1000.0).ceil() as u32;
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    let hours = minutes / 60;
    let minutes = minutes % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn get_double_vec_of_file_paths() -> Vec<Vec<String>> {
    vec![
        vec![
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_bgclar1.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_bgclar2.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_bgclar3.mp3".to_string(),
        ],
        vec![
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_clar1.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_clar2.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_clar3.mp3".to_string(),
        ],
        vec![
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_piano1.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_piano2.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_piano3.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_piano4.mp3".to_string(),
        ],
        vec![
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_rythmn1.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_rythmn2.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_rythmn3.mp3".to_string(),
            "/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/op_rythmn4.mp3".to_string(),
        ],
    ]
}

fn run(args: &ArgMatches) -> Result<i32> {
    let file_path = args.get_one::<String>("INPUT").unwrap();

    println!("file_path: {:?}", file_path);

    // let mut prot = prot::Prot::new_from_file_paths(&get_double_vec_of_file_paths());

    // // loop 10 times
    // for _ in 0..10 {
    //     println!("Duration: {}", prot.get_duration());
    //     prot.refresh_tracks();
    // }

    // prot = prot::Prot::new(&"/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/export4.prot".to_string());

    // // loop 10 times
    // for _ in 0..10 {
    //     println!("Duration: {}", prot.get_duration());
    //     prot.refresh_tracks();
    // }

    // return Ok(0);

    // If file is not a .mka file, return an error
    if !(file_path.ends_with(".prot") || file_path.ends_with(".mka")) {
        panic!("File is not a .prot file");
    }

    let mut player = player::Player::new(file_path);

    player.play();

    let mut loop_iteration = 0;
    while !player.is_finished() {
        // if loop_iteration > 0 {
        //     // println!("Get time: {}", player.get_time());
        //     println!("Refreshing tracks at {}", format_time(player.get_time() * 1000.0));
        //     println!("Duration: {}", format_time(player.get_duration() * 1000.0));
        //     // Measure the time it takes to refresh tracks
        //     let start = std::time::Instant::now();
        //     player.refresh_tracks();
        //     let duration = start.elapsed();
        //     println!("Refreshed tracks in {}ms", duration.as_millis());
        //     // println!("Get time: {}", player.get_time());
        // }
        // loop_iteration += 1;

        // if loop_iteration > 0 {
        //     if !player.is_paused() {
        //         player.pause();
        //     } else {
        //         // Set volume to random number between 0.0 and 1.0
        //         let volume = rand::thread_rng().gen_range(0.3..1.0);
        //         println!("Setting volume to {}", volume);
        //         player.set_volume(volume);
        //         println!("Get volume: {}", player.get_volume());
        //         player.play();
        //         println!("Starting playback at {}", format_time(player.get_time() * 1000.0));
        //     }
        // }
        // loop_iteration += 1;


        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    while !player.is_finished() {
        println!(
            "{} / {}",
            format_time(player.get_time() * 1000.0),
            format_time(player.get_duration() * 1000.0)
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(0)
}
