use crate::Tag;

/// Helper to return colorful classes for tag badges (standard Tailwind classes only)
pub fn get_tag_badge_class(tag: &Tag) -> &'static str {
    match tag {
        Tag::Rust => "bg-orange-500/15 text-orange-400 border border-orange-500/20",
        Tag::Technology => "bg-blue-500/15 text-blue-400 border border-blue-500/20",
        Tag::Action => "bg-red-500/15 text-red-400 border border-red-500/20",
        Tag::SciFi => "bg-purple-500/15 text-purple-400 border border-purple-500/20",
        Tag::Comedy => "bg-yellow-550/15 text-yellow-500 border border-yellow-500/20",
        Tag::Drama => "bg-emerald-500/15 text-emerald-400 border border-emerald-500/20",
        Tag::Documentary => "bg-teal-500/15 text-teal-400 border border-teal-500/20",
        Tag::Other(_) => "bg-zinc-800/60 text-zinc-400 border border-zinc-700/40",
    }
}

/// Helper to generate deterministic gradient classes based on a string's hash
pub fn get_gradient_class(name: &str) -> &'static str {
    let hash = name.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let gradients = [
        "from-red-650 to-amber-500",
        "from-purple-600 to-indigo-600",
        "from-blue-500 to-cyan-500",
        "from-emerald-500 to-teal-500",
        "from-pink-500 to-rose-500",
        "from-fuchsia-600 to-pink-500",
        "from-orange-500 to-yellow-500",
        "from-violet-600 to-purple-500",
    ];
    gradients[(hash as usize) % gradients.len()]
}