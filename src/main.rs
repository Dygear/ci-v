use ci_v::Radio;
use log::info;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("CI-V Controller for ICOM ID-52A Plus");
    println!("=====================================");

    // Auto-discover and connect to the radio.
    let mut radio = match Radio::auto_connect() {
        Ok(r) => {
            println!("Connected to radio.");
            r
        }
        Err(e) => {
            eprintln!("Failed to connect: {e}");
            eprintln!();
            eprintln!("Troubleshooting:");
            eprintln!("  1. Connect the ID-52A Plus via USB-C");
            eprintln!("  2. Ensure the Following Settings on the Radio:");
            eprintln!("     Menu > Set > Function");
            eprintln!("         CI-V > CI-V Address = B4");
            eprintln!("         CI-V > CI-V Buad Rate (SP Jack) = Auto");
            eprintln!("         CI-V > CI-V Transceive = ON");
            eprintln!("         CI-V > CI-V USB/Bluetooth->Remote Transceive Address = 00");
            eprintln!("         USB Connect = Serialport");
            eprintln!("         USB Serialport Function = CI-V (Echo Back ON)");
            eprintln!("  3. Ensure the ICOM USB driver is installed");
            std::process::exit(1);
        }
    };

    // Read and display the current frequency.
    match radio.read_frequency() {
        Ok(freq) => {
            info!("frequency: {freq}");
            println!("Frequency: {freq}");
        }
        Err(e) => eprintln!("Failed to read frequency: {e}"),
    }

    // Read and display the current mode.
    match radio.read_mode() {
        Ok(mode) => {
            info!("mode: {mode}");
            println!("Mode:      {mode}");
        }
        Err(e) => eprintln!("Failed to read mode: {e}"),
    }

    // Read and display the S-meter.
    match radio.read_s_meter() {
        Ok(level) => {
            info!("S-meter: {level}");
            println!("S-meter:   {level}/255");
        }
        Err(e) => eprintln!("Failed to read S-meter: {e}"),
    }

    // Read and display the AF level.
    match radio.read_af_level() {
        Ok(level) => {
            info!("AF level: {level}");
            println!("AF Level:  {level}/255");
        }
        Err(e) => eprintln!("Failed to read AF level: {e}"),
    }

    // Read and display the squelch level.
    match radio.read_squelch() {
        Ok(level) => {
            info!("squelch: {level}");
            println!("Squelch:   {level}/255");
        }
        Err(e) => eprintln!("Failed to read squelch: {e}"),
    }
}
