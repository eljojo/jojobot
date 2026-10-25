use jojobot_adapters::dolt::memory::DoltMemory;
use tracing::instrument::WithSubscriber;

#[derive(Clone, Default)]
struct Kept(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Kept {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Kept {
    type Writer = Kept;

    fn make_writer(&'a self) -> Kept {
        self.clone()
    }
}

pub(super) async fn boot(memory: &DoltMemory) -> String {
    let kept = Kept::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(kept.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .finish();
    jojobot::wiring::field_links::boot_field_links(memory)
        .with_subscriber(subscriber)
        .await;
    String::from_utf8_lossy(&kept.0.lock().unwrap()).into_owned()
}
