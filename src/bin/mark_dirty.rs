// Helper tool to mark files as dirty for testing
use anyhow::Result;
use vibefs::db::MetadataStore;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: mark_dirty <repo_path> <file_path>...");
        std::process::exit(1);
    }

    let repo_path = &args[1];
    let metadata_path = format!("{}/.vibe/metadata.db", repo_path);

    let metadata = MetadataStore::open(&metadata_path)?;

    for path in &args[2..] {
        metadata.mark_dirty(path)?;
        println!("Marked dirty: {}", path);
    }

    Ok(())
}
