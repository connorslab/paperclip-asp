fn main() {
	if std::env::args().nth(1).as_deref() != Some("--experimental-signet") {
		eprintln!("Requires --experimental-signet; test coins only"); std::process::exit(1);
	}
	if let Err(e) = bitcoin_ext::covenant_io::execute("asp") {
		eprintln!("{}",e); std::process::exit(1);
	}
}
