fn main() {
    let mut f = 1 << 2;
    loop {
        let (y, n) = how_many_skips(f);
        if n <= 4 {
            println!("f=0x{f:04X} -> y={y:>6} n={n:>6}");
        }

        let Some(r) = f.checked_add(1 << 2) else {
            break;
        };
        f = r;
    }
}

fn how_many_skips(freq: u16) -> (usize, usize) {
    let initial_phase: u16 = 0;

    let mut yes: usize = 0;
    let mut no: usize = 0;
    let mut phase = initial_phase;
    loop {
        let next_phase = phase.wrapping_add(freq >> 2);
        if (phase ^ next_phase) & 0x4000 != 0 {
            yes += 1;
        } else {
            no += 1;
        }
        phase = next_phase;
        if phase == initial_phase {
            break;
        }
    }

    (yes, no)
}
