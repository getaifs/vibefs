//! Auto-naming for sessions using docker-style adjective-noun combinations.
//!
//! Word lists derived from https://github.com/bearjaws/docker-names

use rand::prelude::IndexedRandom;
use std::collections::HashSet;
use std::path::Path;

/// Adjectives for session naming (from docker-names)
pub const ADJECTIVES: &[&str] = &[
    "admiring", "adoring", "affectionate", "agitated", "amazing", "angry", "awesome",
    "beautiful", "blissful", "bold", "boring", "brave", "busy", "charming", "clever",
    "compassionate", "competent", "condescending", "confident", "cool", "cranky", "crazy",
    "dazzling", "determined", "distracted", "dreamy", "eager", "ecstatic", "elastic",
    "elated", "elegant", "eloquent", "epic", "exciting", "fervent", "festive", "flamboyant",
    "focused", "friendly", "frosty", "funny", "gallant", "gifted", "goofy", "gracious",
    "great", "happy", "hardcore", "heuristic", "hopeful", "hungry", "infallible", "inspiring",
    "intelligent", "interesting", "jolly", "jovial", "keen", "kind", "laughing", "loving",
    "lucid", "magical", "modest", "musing", "mystifying", "naughty", "nervous", "nice",
    "nifty", "nostalgic", "objective", "optimistic", "peaceful", "pedantic", "pensive",
    "practical", "priceless", "quirky", "quizzical", "recursing", "relaxed", "reverent",
    "romantic", "sad", "serene", "sharp", "silly", "sleepy", "stoic", "strange", "stupefied",
    "suspicious", "sweet", "tender", "thirsty", "trusting", "unruffled", "upbeat", "vibrant",
    "vigilant", "vigorous", "wizardly", "wonderful", "xenodochial", "youthful", "zealous", "zen",
];

/// Nouns for session naming (scientist surnames from docker-names)
pub const NOUNS: &[&str] = &[
    "agnesi", "albattani", "allen", "almeida", "antonelli", "archimedes", "ardinghelli",
    "aryabhata", "austin", "babbage", "banach", "banzai", "bardeen", "bartik", "bassi",
    "beaver", "bell", "benz", "bhabha", "bhaskara", "black", "blackburn", "blackwell",
    "bohr", "booth", "borg", "bose", "bouman", "boyd", "brahmagupta", "brattain", "brown",
    "buck", "burnell", "cannon", "carson", "cartwright", "carver", "cerf", "chandrasekhar",
    "chaplygin", "chatelet", "chatterjee", "chaum", "chebyshev", "clarke", "cohen", "colden",
    "cori", "cray", "curie", "curran", "darwin", "davinci", "dewdney", "dhawan", "diffie",
    "dijkstra", "dirac", "driscoll", "dubinsky", "easley", "edison", "einstein", "elbakyan",
    "elgamal", "elion", "ellis", "engelbart", "euclid", "euler", "faraday", "feistel",
    "fermat", "fermi", "feynman", "franklin", "gagarin", "galileo", "galois", "ganguly",
    "gates", "gauss", "germain", "goldberg", "goldstine", "goldwasser", "golick", "goodall",
    "gould", "greider", "grothendieck", "haibt", "hamilton", "haslett", "hawking", "heisenberg",
    "hellman", "hermann", "herschel", "hertz", "heyrovsky", "hodgkin", "hofstadter", "hoover",
    "hopper", "hugle", "hypatia", "ishizaka", "jackson", "jang", "jemison", "jennings",
    "jepsen", "johnson", "joliot", "jones", "kalam", "kapitsa", "kare", "keldysh", "keller",
    "kepler", "khayyam", "khorana", "kilby", "kirch", "knuth", "kowalevski", "lalande",
    "lamarr", "lamport", "leakey", "leavitt", "lederberg", "lehmann", "lewin", "lichterman",
    "liskov", "lovelace", "lumiere", "mahavira", "margulis", "matsumoto", "maxwell", "mayer",
    "mccarthy", "mcclintock", "mclaren", "mclean", "mcnulty", "meitner", "mendel", "mendeleev",
    "meninsky", "merkle", "mestorf", "mirzakhani", "montalcini", "moore", "morse", "moser",
    "murdock", "napier", "nash", "neumann", "newton", "nightingale", "nobel", "noether",
    "northcutt", "noyce", "panini", "pare", "pascal", "pasteur", "payne", "perlman", "pike",
    "poincare", "poitras", "proskuriakova", "ptolemy", "raman", "ramanujan", "rhodes", "ride",
    "ritchie", "robinson", "roentgen", "rosalind", "rubin", "saha", "sammet", "sanderson",
    "satoshi", "shamir", "shannon", "shaw", "shirley", "shockley", "shtern", "sinoussi",
    "snyder", "solomon", "spence", "stonebraker", "sutherland", "swanson", "swartz", "swirles",
    "taussig", "tesla", "tharp", "thompson", "torvalds", "tu", "turing", "varahamihira",
    "vaughan", "villani", "visvesvaraya", "volhard", "wescoff", "wilbur", "wiles", "williams",
    "williamson", "wilson", "wing", "wozniak", "wright", "wu", "yalow", "yonath", "zhukovsky",
];

/// Generate a random session name in the format "adjective-noun"
pub fn generate_name() -> String {
    let mut rng = rand::rng();
    let adjective = ADJECTIVES.choose(&mut rng).unwrap_or(&"eager");
    let noun = NOUNS.choose(&mut rng).unwrap_or(&"turing");
    format!("{}-{}", adjective, noun)
}

/// Generate a session name for an agent in the format "adjective-agent"
pub fn generate_agent_name(agent: &str) -> String {
    let mut rng = rand::rng();
    let adjective = ADJECTIVES.choose(&mut rng).unwrap_or(&"eager");
    format!("{}-{}", adjective, agent)
}

/// Generate a unique session name that doesn't conflict with existing sessions
pub fn generate_unique_name(sessions_dir: &Path) -> String {
    let existing: HashSet<String> = if sessions_dir.exists() {
        std::fs::read_dir(sessions_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|name| !name.contains("_snapshot_"))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashSet::new()
    };

    // Try to generate a unique name
    for _ in 0..100 {
        let name = generate_name();
        if !existing.contains(&name) {
            return name;
        }
    }

    // Fallback: append number to make unique
    let base = generate_name();
    for i in 2..1000 {
        let name = format!("{}-{}", base, i);
        if !existing.contains(&name) {
            return name;
        }
    }

    // Ultimate fallback with timestamp
    format!("{}-{}", base, chrono::Utc::now().timestamp())
}

/// Generate a unique agent session name
pub fn generate_unique_agent_name(agent: &str, sessions_dir: &Path) -> String {
    let existing: HashSet<String> = if sessions_dir.exists() {
        std::fs::read_dir(sessions_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|name| !name.contains("_snapshot_"))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashSet::new()
    };

    // Try to generate a unique name
    for _ in 0..100 {
        let name = generate_agent_name(agent);
        if !existing.contains(&name) {
            return name;
        }
    }

    // Fallback: append number
    let base = generate_agent_name(agent);
    for i in 2..1000 {
        let name = format!("{}-{}", base, i);
        if !existing.contains(&name) {
            return name;
        }
    }

    format!("{}-{}", base, chrono::Utc::now().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_name_format() {
        let name = generate_name();
        assert!(name.contains('-'), "Name should contain hyphen: {}", name);
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 2, "Name should have two parts: {}", name);
        assert!(ADJECTIVES.contains(&parts[0]), "First part should be adjective");
        assert!(NOUNS.contains(&parts[1]), "Second part should be noun");
    }

    #[test]
    fn test_generate_agent_name() {
        let name = generate_agent_name("claude");
        assert!(name.ends_with("-claude"), "Should end with agent name: {}", name);
        let parts: Vec<&str> = name.split('-').collect();
        assert!(ADJECTIVES.contains(&parts[0]), "First part should be adjective");
    }

    #[test]
    fn test_generate_unique_name_no_existing() {
        let temp_dir = TempDir::new().unwrap();
        let name = generate_unique_name(temp_dir.path());
        assert!(name.contains('-'));
    }

    #[test]
    fn test_generate_unique_name_with_existing() {
        let temp_dir = TempDir::new().unwrap();

        // Create some existing session directories
        std::fs::create_dir(temp_dir.path().join("eager-turing")).unwrap();
        std::fs::create_dir(temp_dir.path().join("happy-einstein")).unwrap();

        // Generate unique names - they should not conflict
        let mut names = HashSet::new();
        for _ in 0..10 {
            let name = generate_unique_name(temp_dir.path());
            assert!(!names.contains(&name), "Generated duplicate name: {}", name);
            names.insert(name);
        }
    }

    #[test]
    fn test_word_list_sizes() {
        // Ensure we have enough variety
        assert!(ADJECTIVES.len() >= 100, "Should have at least 100 adjectives");
        assert!(NOUNS.len() >= 200, "Should have at least 200 nouns");
    }
}
