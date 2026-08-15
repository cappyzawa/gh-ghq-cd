mod app;
mod command;
mod environment;

fn main() -> anyhow::Result<()> {
    app::run()
}
