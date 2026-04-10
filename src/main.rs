use clap::Parser;
use clap::ValueEnum;
use clap_num::maybe_hex;
use cody_emulator::assembler::disassemble;
use cody_emulator::frontend;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Binary file
    file: PathBuf,

    /// Load the binary file as a cartridge, expects the file to have a cartridge header
    #[arg(long, default_value_t = false)]
    as_cartridge: bool,

    /// Load address, default value is 0xE000
    #[arg(long, value_parser=maybe_hex::<u16>)]
    load_address: Option<u16>,

    /// Override Reset Vector (0xFFFC)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    reset_vector: Option<u16>,

    /// Override Interrupt Vector (0xFFFE)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    irq_vector: Option<u16>,

    /// Override Non-maskable Interrupt Vector (0xFFFA)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    nmi_vector: Option<u16>,

    /// Path of file used to fill the UART1 receive buffer with bytes
    #[arg(long)]
    uart1_source: Option<PathBuf>,

    /// This option will normalize newlines when reading text data for the UART.
    ///
    /// Use this when your input text file might have CRLF-style line endings or to make sure it works for CodyBASIC's LOAD 1,0 command.
    #[arg(long, default_value_t = false)]
    fix_newlines: bool,

    /// Emulate the keyboard by physically mapping the cody keyboard, without respecting the host's layout.
    #[arg(long, default_value_t = false)]
    physical_keyboard: bool,

    /// Run the cpu as fast as possible.
    #[arg(long, default_value_t = false)]
    fast: bool,

    /// Latency bias preset for the target buffer length.
    ///
    /// Lower values bias toward lower latency (more aggressive), while higher values bias toward smoother playback.
    #[arg(long, value_enum, conflicts_with_all = ["audio_disable", "audio_silent"])]
    audio_latency: Option<AudioLatencyPreset>,

    /// Catch-up trigger strictness preset.
    ///
    /// Lower values trigger catch-up earlier (more aggressive), while higher values trigger later (more relaxed).
    /// If you're annoyed by some weird audio artifacts, try relaxing this value.
    #[arg(long, value_enum, conflicts_with_all = ["audio_disable", "audio_silent"])]
    audio_catchup: Option<AudioCatchupStrictnessPreset>,

    /// Disable initial catch-up sensitivity heuristic for audio buffering.
    ///
    /// By default, the emulator will be more aggressive about triggering catch-up when the audio buffer is initially filling up, to reduce latency early.
    /// Only affects first catch-up.
    #[arg(long, default_value_t = false, conflicts_with_all = ["audio_disable", "audio_silent"])]
    audio_no_initial_catchup: bool,

    /// Use the lightweight linear output resampler instead of the default cubic resampler.
    #[arg(long, default_value_t = false, conflicts_with_all = ["audio_disable", "audio_silent"])]
    audio_resampler_fast: bool,

    /// Disable audio output (keeps the audio engine running for authentic emulation of audio register behavior).
    #[arg(long, default_value_t = false, conflicts_with = "audio_disable", short = 's')]
    audio_silent: bool,

    /// Disable audio entirely (disables everything related to audio, including the MMIO device and engine).
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = [
            "audio_silent",
            "audio_no_initial_catchup",
            "audio_resampler_fast",
            "audio_latency",
            "audio_catchup"
        ]
    )]
    audio_disable: bool,

    /// Each time this option is added increases the default logging level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum AudioLatencyPreset {
    Low,
    Default,
    Medium,
    High,
    Generous,
}

impl AudioLatencyPreset {
    const fn latency_bias_q10(self) -> u16 {
        match self {
            Self::Low => 896,
            Self::Default => 1024,
            Self::Medium => 1152,
            Self::High => 1280,
            Self::Generous => 1408,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum AudioCatchupStrictnessPreset {
    Relaxed,
    Default,
    Strict,
    VeryStrict,
}

impl AudioCatchupStrictnessPreset {
    const fn strictness_q10(self) -> u16 {
        match self {
            Self::Relaxed => 1280,
            Self::Default => 1024,
            Self::Strict => 896,
            Self::VeryStrict => 800,
        }
    }
}

pub fn main() {
    let cli = Cli::parse();

    // To change the log level, set the `RUST_LOG` environment variable. See the `env_logger`
    // documentation for more information.
    unsafe {
        if env::var(env_logger::DEFAULT_FILTER_ENV).is_err() {
            match cli.verbose {
                0 => env::set_var(env_logger::DEFAULT_FILTER_ENV, "warn"),
                1 => env::set_var(env_logger::DEFAULT_FILTER_ENV, "info"),
                2 => env::set_var(env_logger::DEFAULT_FILTER_ENV, "debug"),
                3.. => env::set_var(env_logger::DEFAULT_FILTER_ENV, "trace"),
            }
        }
    }
    env_logger::init();

    frontend::start(
        &cli.file,
        cli.as_cartridge,
        cli.load_address,
        cli.reset_vector,
        cli.irq_vector,
        cli.nmi_vector,
        cli.uart1_source.as_deref(),
        cli.fix_newlines,
        cli.physical_keyboard,
        cli.fast,
        cli.audio_latency
            .unwrap_or(AudioLatencyPreset::Default)
            .latency_bias_q10(),
        cli.audio_catchup
            .unwrap_or(AudioCatchupStrictnessPreset::Default)
            .strictness_q10(),
        cli.audio_no_initial_catchup,
        cli.audio_resampler_fast,
        cli.audio_silent,
        cli.audio_disable
    );
}

#[allow(dead_code)]
fn dis(data: &[u8]) {
    let instructions = disassemble(data);
    for insn in instructions {
        println!("{insn}");
    }
}
