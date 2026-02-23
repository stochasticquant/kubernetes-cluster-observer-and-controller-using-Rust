pub fn run() -> anyhow::Result<()> {
    println!("kube-devops version {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
