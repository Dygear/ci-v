# What is CI-V?
CI-V is a command-line tool for interacting with the CI-V protocol enabled devices, such as the ICOM ID-52Plus. This code is specific to the ICOM ID-52Plus and that radio's implementation of the protocol at this time. It allows you to control the radio's:
* Frequency,
* Mode,
* Channel Width,
* Transmit Power,
* Tx and Rx Signal, and
* Repeater offset.

While also showing the radio's:
* S-Meter,
* Volume Level, and
* Sqlelch setting;
* With an added bonus of GPS location.

The radio is controlled via the keyboard shortcuts, which are listed below:
* `F` - **F**requency
* `M` - **M**ode
* `W` - Channel **W**idth
* `V` - **V**FO Select
* `A` - **A**udio Volume
    * `+` - Increase Volume
    * `-` - Decrease Volume
    * `0` - Toggle Mute
* `S` - **S**quelch
* `P` - **P**ower
* `O` - Repeater **O**ffset
* `T` - **T**ransmit Tone
* `R` - **R**ecive Tone

# Install
## Windows
For Windows, you must have [Rust installed](https://rustup.rs/) and added to your PATH environment variable, that also includes Visual Studio and the "Desktop Development with C++" Build Tools in Workloads of the Visual Studio Installer.

## Linux
For Linux, you must have [Rust installed](https://rustup.rs/) and added to your PATH environment variable. You MAY also need to add your user to the `dialout` group as follows:

```bash
sudo usermod -a -G dialout $USER
```

You will likely also need to log out and back in for the changes to take effect.

## macOS
For macOS, you must have [Rust installed](https://rustup.rs/) and added to your PATH environment variable.

## All
```bash
git clone https://github.com/Dygear/ci-v.git
cd ci-v
cargo build --release
```
