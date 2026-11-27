fn main() {
    for max in 1u16..=254 {
        let limit = 255 - (256 % (max + 1));
        println!("max={max:>3}: limit={limit:>3} last={:>3}", limit % (max + 1));
    }
}
