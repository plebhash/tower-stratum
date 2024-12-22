use roles_logic_sv2::utils::Id;

pub mod service;
pub mod tcp;

/// alias for [`roles_logic_sv2::utils::Id`], which is a generator of unique `u32` Ids
/// (by simply incrementing the last `u32`)
pub type ClientIdGenerator = Id;
