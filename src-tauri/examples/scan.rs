//! Diagnostic: print what each platform adapter detects on this machine.
//! Run with `cargo run --example scan`.
use vanta_lib::platforms;

fn main() {
    for a in platforms::all_adapters() {
        let d = a.detect();
        println!(
            "== {} detected={} connected={} account={:?}",
            a.name(),
            d.detected,
            d.connected,
            d.account.is_some()
        );
        for line in &d.details {
            println!("   {line}");
        }
        match a.discover() {
            Ok(games) => {
                for g in games {
                    println!(
                        "   - {} [{}] installed={} playtime={:?}min cover_local={} hero_local={}",
                        g.name,
                        g.external_id,
                        g.installed,
                        g.platform_playtime_min,
                        g.artwork.cover_local.is_some(),
                        g.artwork.hero_local.is_some()
                    );
                }
            }
            Err(e) => println!("   discover: {e}"),
        }
    }
    println!(
        "== runners: {:?}",
        platforms::runners::detect()
            .iter()
            .map(|r| &r.name)
            .collect::<Vec<_>>()
    );
    println!(
        "== desktop games: {:?}",
        platforms::desktop::discover()
            .iter()
            .map(|g| &g.name)
            .collect::<Vec<_>>()
    );
    println!(
        "== steam downloads: {}",
        platforms::steam::SteamAdapter::system().downloads().len()
    );
}
