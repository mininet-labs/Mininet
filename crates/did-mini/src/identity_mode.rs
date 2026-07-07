//! Identity modes (founder decision, 2026-07-07): the vocabulary for how a
//! human may show up on Mininet.
//!
//! Human status itself stays private and one-per-human (SPEC-02 P2,
//! pending). Everything in this enum is built *on top* of a human-root, by
//! the human's own choice — none of it changes personhood status, and none
//! of it multiplies or dilutes a vote.
//!
//! | Mode | What it is | Status here |
//! |---|---|---|
//! | [`IdentityMode::HumanRoot`] | The private, cold, never-public-by-default root identity — an un-delegated `did:mini` [`crate::Kel`]. | implemented |
//! | [`IdentityMode::BaseDevice`] | The user's recommended main/static device for hosting, storage, and seeding. | implemented — [`crate::BaseDeviceRole`] |
//! | [`IdentityMode::DeviceDid`] | A delegated device, capability-scoped to its human-root. | implemented — [`crate::delegation`] |
//! | [`IdentityMode::PublicWall`] | A chosen public-facing profile, published under any DID the user picks. | implemented — `mini-social::PublicWall` |
//! | [`IdentityMode::PseudonymProfile`] | A context identity: an independent `did:mini` root run without publishing a linkage to any other identity. | implemented — [`crate::Controller::incept_pairwise_pseudonym`] (SPEC-01 §10) derives one deterministically per context, so it's a function call rather than a hand-managed seed |
//! | [`IdentityMode::AnonymousAction`] | A nullifier/ZK-proved action revealing nothing beyond "some verified human did this once." | `pending` — SPEC-02 `PersonhoodOracle` |
//!
//! This is intentionally a plain, unsigned, non-wire enum: it is vocabulary
//! for docs, UI, and tests, not a protocol message. The actual guarantees
//! each mode makes live in the modules named above.

/// One of the ways a human may participate, from fully private to fully
/// anonymous. See the module docs for what each mode is and is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IdentityMode {
    /// Private, cold, never public by default.
    HumanRoot,
    /// The recommended main device for hosting/storage/seeding — operational
    /// infrastructure, not political power.
    BaseDevice,
    /// A capability-scoped delegate of exactly one human-root.
    DeviceDid,
    /// A voluntary public disclosure surface, not the identity root.
    PublicWall,
    /// An independent `did:mini` root, unlinkable by default.
    PseudonymProfile,
    /// A nullifier/ZK action proving humanness without identity.
    AnonymousAction,
}

impl IdentityMode {
    /// All modes, in the order documented above.
    pub const ALL: [IdentityMode; 6] = [
        IdentityMode::HumanRoot,
        IdentityMode::BaseDevice,
        IdentityMode::DeviceDid,
        IdentityMode::PublicWall,
        IdentityMode::PseudonymProfile,
        IdentityMode::AnonymousAction,
    ];

    /// Whether this mode has working, tested code in the Mininet workspace
    /// today (the same honesty convention `README.md`'s alpha note uses:
    /// `pending` means the concept is designed but not yet implemented).
    pub const fn implemented(self) -> bool {
        !matches!(self, IdentityMode::AnonymousAction)
    }

    /// One-line description of the guarantee this mode makes.
    pub const fn describe(self) -> &'static str {
        match self {
            IdentityMode::HumanRoot => "private, cold, never public by default",
            IdentityMode::BaseDevice => "operational infrastructure, not political power",
            IdentityMode::DeviceDid => "capability-scoped delegate of exactly one human-root",
            IdentityMode::PublicWall => {
                "voluntary public disclosure surface, not the identity root"
            }
            IdentityMode::PseudonymProfile => "an independent did:mini root, unlinkable by default",
            IdentityMode::AnonymousAction => {
                "nullifier/ZK action proving humanness without identity"
            }
        }
    }

    /// Every mode carries the human's status exactly zero or one times, never
    /// more: no mode in this enum multiplies a vote, a score, or a rank. This
    /// is a documentation-level constant `true` for every variant — encoded
    /// as a function so a future variant that *would* break this must edit
    /// this match and think about it.
    pub const fn never_multiplies_standing(self) -> bool {
        match self {
            IdentityMode::HumanRoot
            | IdentityMode::BaseDevice
            | IdentityMode::DeviceDid
            | IdentityMode::PublicWall
            | IdentityMode::PseudonymProfile
            | IdentityMode::AnonymousAction => true,
        }
    }
}
