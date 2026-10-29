//! Experimental native storage conformance. No production writer or executor.
//! Signatures authenticate retained admission evidence, not TigerBeetle itself.
//! Native transport is restricted to an explicitly selected loopback test cluster.
pub mod accounting;
pub mod durable;
pub mod evidence;
#[cfg(feature = "native-conformance")]
pub mod native;
