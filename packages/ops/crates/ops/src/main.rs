//! Ops: the LCARS daemon. Holds one Herdr socket connection per host,
//! merges the streams, and relays key actions. See docs/HANDOFF.md.

fn main() {
    println!("ops {}", env!("CARGO_PKG_VERSION"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_named_ops() {
        assert_eq!(env!("CARGO_PKG_NAME"), "ops");
    }
}
