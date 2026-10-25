// SPDX-License-Identifier: MPL-2.0
//! Small length-prefixed reference framing model. Not a claim that Wharf's
//! wire protocol uses this framing. Callers must supply the correct adapter.
use anyhow::{bail, Result};
#[derive(Default)]
pub struct Frames {
    pending: Vec<u8>,
    failed: bool,
}
impl Frames {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>> {
        if self.failed {
            bail!("framer is failed");
        }
        let mut frames = Vec::new();
        for byte in chunk {
            self.pending.push(*byte);
            if self.pending.len() >= 4 {
                let len = u32::from_be_bytes(self.pending[..4].try_into().unwrap()) as usize;
                if len > 1024 * 1024 {
                    self.failed = true;
                    self.pending.clear();
                    bail!("frame exceeds 1 MiB");
                }
                if self.pending.len() == len + 4 {
                    frames.push(self.pending.split_off(4));
                    self.pending.clear();
                }
            }
        }
        Ok(frames)
    }
    pub fn finish(&self) -> Result<()> {
        if self.failed || !self.pending.is_empty() {
            bail!("failed/truncated stream");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_fragmentation_and_coalescing_has_same_frames() {
        let wire = [0, 0, 0, 3, b'a', b'b', b'c', 0, 0, 0, 1, b'd'];
        for split in 0..=wire.len() {
            let mut f = Frames::default();
            let mut out = f.push(&wire[..split]).unwrap();
            out.extend(f.push(&wire[split..]).unwrap());
            f.finish().unwrap();
            assert_eq!(out, vec![b"abc".to_vec(), b"d".to_vec()]);
        }
        let mut f = Frames::default();
        let mut out = vec![];
        for byte in wire {
            out.extend(f.push(&[byte]).unwrap());
        }
        assert_eq!(out.len(), 2);
    }
    #[test]
    fn malformed_and_truncated_never_pass() {
        let mut f = Frames::default();
        f.push(&[0, 0]).unwrap();
        assert!(f.finish().is_err());
        let mut f = Frames::default();
        assert!(f.push(&[255, 255, 255, 255]).is_err());
        assert!(f.push(&[]).is_err());
    }
}
