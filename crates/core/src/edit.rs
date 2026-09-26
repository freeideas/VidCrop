//! The edit model: what to crop and which time ranges to drop. Pure data, no I/O.

use serde::{Deserialize, Serialize};

/// Crop rectangle in source pixels, in displayed (rotation-applied) orientation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// A time range in seconds, `start < end`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub start: f64,
    pub end: f64,
}

impl Range {
    pub fn len(&self) -> f64 {
        self.end - self.start
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    pub source: String,
    /// `None` means "keep the whole frame".
    pub crop: Option<Rect>,
    /// Sorted, non-overlapping, merged.
    pub deleted: Vec<Range>,
}

/// Smallest crop size we allow, in pixels.
pub const MIN_CROP: u32 = 16;

impl Edit {
    pub fn new(source: impl Into<String>) -> Self {
        Edit { source: source.into(), crop: None, deleted: Vec::new() }
    }

    /// Clamps the rectangle to the frame and rounds everything to even numbers,
    /// which 4:2:0 video requires. A rectangle covering the whole frame becomes `None`.
    pub fn set_crop(&mut self, rect: Option<Rect>, frame_w: u32, frame_h: u32) {
        self.crop = rect.and_then(|r| normalize_crop(r, frame_w, frame_h));
    }

    pub fn delete_range(&mut self, start: f64, end: f64, duration: f64) {
        let (s, e) = (start.max(0.0), end.min(duration));
        if e - s <= 0.0 {
            return;
        }
        self.deleted.push(Range { start: s, end: e });
        self.deleted = merge(std::mem::take(&mut self.deleted));
    }

    pub fn restore_range(&mut self, start: f64, end: f64) {
        let mut out = Vec::new();
        for r in &self.deleted {
            if r.end <= start || r.start >= end {
                out.push(*r);
                continue;
            }
            if r.start < start {
                out.push(Range { start: r.start, end: start });
            }
            if r.end > end {
                out.push(Range { start: end, end: r.end });
            }
        }
        self.deleted = out;
    }

    /// The parts of `[0, duration]` that survive the cuts. Slivers under 1 ms are dropped.
    pub fn kept(&self, duration: f64) -> Vec<Range> {
        let mut out = Vec::new();
        let mut t = 0.0;
        for r in &self.deleted {
            if r.start > t {
                out.push(Range { start: t, end: r.start });
            }
            t = t.max(r.end);
        }
        if duration > t {
            out.push(Range { start: t, end: duration });
        }
        out.retain(|r| r.len() > 0.001);
        out
    }

    pub fn kept_duration(&self, duration: f64) -> f64 {
        self.kept(duration).iter().map(Range::len).sum()
    }

    pub fn is_noop(&self) -> bool {
        self.crop.is_none() && self.deleted.is_empty()
    }
}

fn even_down(v: u32) -> u32 {
    v & !1
}

fn normalize_crop(r: Rect, fw: u32, fh: u32) -> Option<Rect> {
    let (fw, fh) = (even_down(fw), even_down(fh));
    let x = even_down(r.x.min(fw.saturating_sub(MIN_CROP)));
    let y = even_down(r.y.min(fh.saturating_sub(MIN_CROP)));
    let w = even_down(r.w.max(MIN_CROP).min(fw - x));
    let h = even_down(r.h.max(MIN_CROP).min(fh - y));
    if x == 0 && y == 0 && w == fw && h == fh {
        None
    } else {
        Some(Rect { x, y, w, h })
    }
}

fn merge(mut v: Vec<Range>) -> Vec<Range> {
    v.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut out: Vec<Range> = Vec::new();
    for r in v {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rs(v: &[(f64, f64)]) -> Vec<Range> {
        v.iter().map(|&(start, end)| Range { start, end }).collect()
    }

    #[test]
    fn delete_merges_and_clamps() {
        let mut e = Edit::new("a.mp4");
        e.delete_range(5.0, 7.0, 10.0);
        e.delete_range(1.0, 2.0, 10.0);
        e.delete_range(6.0, 12.0, 10.0);
        assert_eq!(e.deleted, rs(&[(1.0, 2.0), (5.0, 10.0)]));
        assert_eq!(e.kept(10.0), rs(&[(0.0, 1.0), (2.0, 5.0)]));
        assert_eq!(e.kept_duration(10.0), 4.0);
    }

    #[test]
    fn restore_splits() {
        let mut e = Edit::new("a.mp4");
        e.delete_range(2.0, 8.0, 10.0);
        e.restore_range(4.0, 5.0);
        assert_eq!(e.deleted, rs(&[(2.0, 4.0), (5.0, 8.0)]));
    }

    #[test]
    fn crop_is_even_clamped_and_full_frame_is_none() {
        let mut e = Edit::new("a.mp4");
        e.set_crop(Some(Rect { x: 101, y: 51, w: 3001, h: 301 }), 1920, 1080);
        assert_eq!(e.crop, Some(Rect { x: 100, y: 50, w: 1820, h: 300 }));
        e.set_crop(Some(Rect { x: 0, y: 0, w: 1920, h: 1080 }), 1920, 1080);
        assert_eq!(e.crop, None);
        e.set_crop(Some(Rect { x: 0, y: 0, w: 1, h: 1 }), 1920, 1080);
        assert_eq!(e.crop, Some(Rect { x: 0, y: 0, w: MIN_CROP, h: MIN_CROP }));
    }
}
