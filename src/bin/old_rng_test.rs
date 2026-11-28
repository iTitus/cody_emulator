use cody_emulator::random::Random;
use itertools::Itertools;

fn main() {
    let ns = all_numbers();

    println!(
        "{:?}",
        ns.iter().copied().map(rng_to_output).collect::<Vec<_>>()
    );

    let mut counts = ns.iter().copied().map(|n| n >> 8).counts();
    for n in 0..24 {
        counts.entry(n).or_insert(0);
    }

    counts
        .iter()
        .sorted_unstable_by_key(|&(&n, &c)| (c, n))
        .for_each(|(n, c)| println!("{n:>2}: {c:>4}"));
}

fn all_numbers() -> Vec<u16> {
    let mut rng = Random::new();
    let initial = rng.next_u16();
    let mut numbers = vec![initial];
    loop {
        let n = rng.next_u16();
        if n == initial {
            break;
        }
        numbers.push(n);
    }
    numbers
}

fn rng_to_output(n: u16) -> u8 {
    let n = (n >> 8) as u8;
    n % 24
}
