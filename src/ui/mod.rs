pub mod tailwind;

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