# cody_emulator

Emulate the [Cody Computer](https://www.codycomputer.org/).

Contains a 65C02 emulator and a re-implementation of the Cody firmware.

## Running from source
```
> cargo run --release -- --help
Usage: cody_emulator [OPTIONS] <FILE>

Arguments:
  <FILE>  Binary file

Options:
      --cartridge <CARTRIDGE>        Load given file as cartridge in addition to binary
      --load-address <LOAD_ADDRESS>  Load address [default: 0xE000]
      --reset-vector <RESET_VECTOR>  Override Reset Vector (0xFFFC)
      --irq-vector <IRQ_VECTOR>      Override Interrupt Vector (0xFFFE)
      --nmi-vector <NMI_VECTOR>      Override Non-maskable Interrupt Vector (0xFFFA)
  -h, --help                         Print help
  -V, --version                      Print version
```

### Examples
Run Cody BASIC: `cargo run --release -- codybasic.bin`
![example_basic.png](docs/example_basic.png)

Run Bitmap example: `cargo run --release -- --load-address=0x2FC --reset-vector=0x300 codybitmap.bin`
![example_bitmap.png](docs/example_bitmap.png)
