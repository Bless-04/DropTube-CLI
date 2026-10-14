/// One profile's integer ratings and additive, normalized tag sets.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct UserMetadata {
    /// Each library-relative video identifier has at most one personal rating.
    pub ratings: HashMap<String, u8>,
    /// Tags belong to this profile, never to the shared media index.
    pub tags: HashMap<String, HashSet<String>>,
}
