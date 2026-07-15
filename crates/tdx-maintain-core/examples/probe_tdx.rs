fn main() {
    use rustdx::tcp::Tdx as _;
    use std::net::SocketAddr;
    use std::time::Instant;

    let tests = [
        ("180.153.18.170:7709", 1u16, "600000"),
        ("180.153.18.170:7709", 0, "000001"),
        ("115.238.90.165:7709", 1, "600000"),
        ("110.41.147.114:7709", 1, "600000"),
    ];
    for (a, market, code) in tests {
        let addr: SocketAddr = a.parse().unwrap();
        let t = Instant::now();
        match rustdx::tcp::Tcp::new_with_ip(&addr) {
            Ok(mut tcp) => {
                let mut kline = rustdx::tcp::stock::Kline::new(market, code, 9, 0, 10);
                match kline.recv_parsed(&mut tcp) {
                    Ok(data) => {
                        let last = data.iter().filter_map(|k| {
                            let y = k.dt.year as i32;
                            if (1990..=2100).contains(&y) {
                                Some(format!("{}-{:02}-{:02}", k.dt.year, k.dt.month, k.dt.day))
                            } else { None }
                        }).last();
                        println!("OK {} m={} {} n={} last={:?} ({:?})", a, market, code, data.len(), last, t.elapsed());
                    }
                    Err(e) => println!("ERR kline {} {} ({:?})", a, e, t.elapsed()),
                }
            }
            Err(e) => println!("ERR handshake {} {} ({:?})", a, e, t.elapsed()),
        }
    }
}
