//! Queue primitives used by the audio runtime.
//!
//! These types intentionally avoid blocking and instead track overrun/underrun
//! counters so real-time audio paths can degrade gracefully.

use crossbeam_queue::ArrayQueue;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A bounded lock-free queue that drops the oldest item on overflow.
#[derive(Clone)]
pub struct LockFreeQueue<T> {
    data: Arc<ArrayQueue<T>>,
    overrun_count: Arc<AtomicU64>,
}

impl<T> fmt::Debug for LockFreeQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockFreeQueue")
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .field("overrun_count", &self.overrun_count())
            .finish()
    }
}

impl<T> LockFreeQueue<T> {
    /// Creates a new lock-free queue with the provided item capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        Self {
            data: Arc::new(ArrayQueue::new(capacity)),
            overrun_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns the maximum number of queued items.
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// Returns the currently queued item count.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns whether the queue currently holds no items.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Pushes an item, dropping one oldest item when the queue is full.
    pub fn push_drop_oldest(&self, value: T) {
        if self.data.force_push(value).is_some() {
            self.overrun_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Pops one item from the front of the queue.
    pub fn pop_front(&self) -> Option<T> {
        self.data.pop()
    }

    /// Drains currently queued items into `out` in pop order.
    pub fn drain_into(&self, out: &mut Vec<T>) {
        out.clear();
        out.reserve(self.len());
        while let Some(item) = self.data.pop() {
            out.push(item);
        }
    }

    /// Returns how many items were dropped due to queue overflow.
    pub fn overrun_count(&self) -> u64 {
        self.overrun_count.load(Ordering::Relaxed)
    }
}

/// A lock-free PCM buffer with overrun/underrun counters.
#[derive(Clone)]
pub struct LockFreePcmRingBuffer {
    data: Arc<ArrayQueue<f32>>,
    overrun_samples: Arc<AtomicU64>,
    underrun_samples: Arc<AtomicU64>,
}

impl fmt::Debug for LockFreePcmRingBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockFreePcmRingBuffer")
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .field("overrun_samples", &self.overrun_samples())
            .field("underrun_samples", &self.underrun_samples())
            .finish()
    }
}

impl LockFreePcmRingBuffer {
    /// Creates a lock-free PCM buffer with fixed sample capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        Self {
            data: Arc::new(ArrayQueue::new(capacity)),
            overrun_samples: Arc::new(AtomicU64::new(0)),
            underrun_samples: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns current buffered sample count.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns the fixed sample capacity.
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// Appends samples, overwriting oldest samples on overflow.
    pub fn push_samples(&self, samples: &[f32]) {
        for &sample in samples {
            if self.data.force_push(sample).is_some() {
                self.overrun_samples.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Pops up to `wanted` samples into `out`; increments underrun when depleted.
    pub fn pop_samples(&self, wanted: usize, out: &mut Vec<f32>) {
        out.clear();
        out.reserve(wanted);

        for _ in 0..wanted {
            if let Some(sample) = self.data.pop() {
                out.push(sample);
            } else {
                self.underrun_samples.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Removes one sample from the front if available.
    pub fn pop_front(&self) {
        let _ = self.data.pop();
    }

    /// Returns total overwritten samples due to overflow.
    pub fn overrun_samples(&self) -> u64 {
        self.overrun_samples.load(Ordering::Relaxed)
    }

    /// Returns total failed reads due to underrun.
    pub fn underrun_samples(&self) -> u64 {
        self.underrun_samples.load(Ordering::Relaxed)
    }
}

/// A fixed-capacity FIFO queue that drops the oldest item on overflow.
#[derive(Debug, Clone)]
pub struct BoundedQueue<T> {
    data: VecDeque<T>,
    capacity: usize,
    overrun_count: u64,
}

impl<T> BoundedQueue<T> {
    /// Creates a new queue with the provided maximum item capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
            overrun_count: 0,
        }
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn front(&self) -> Option<&T> {
        self.data.front()
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.data.pop_front()
    }

    /// Pushes an item, dropping the oldest queued item when full.
    pub fn push_drop_oldest(&mut self, value: T) {
        if self.data.len() >= self.capacity {
            let _ = self.data.pop_front();
            self.overrun_count = self.overrun_count.saturating_add(1);
        }
        self.data.push_back(value);
    }

    /// Drains all items in FIFO order into a new vector.
    pub fn drain(&mut self) -> Vec<T> {
        self.data.drain(..).collect()
    }

    /// Returns how many items were dropped due to queue overflow.
    pub const fn overrun_count(&self) -> u64 {
        self.overrun_count
    }
}

/// A fixed-capacity PCM sample buffer with overrun/underrun accounting.
#[derive(Debug, Clone)]
pub struct PcmRingBuffer {
    data: VecDeque<f32>,
    capacity: usize,
    overrun_samples: u64,
    underrun_samples: u64,
}

impl PcmRingBuffer {
    /// Creates a sample buffer with the provided maximum sample capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
            overrun_samples: 0,
            underrun_samples: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Appends samples, discarding oldest samples if capacity is exceeded.
    pub fn push_samples(&mut self, samples: &[f32]) {
        for &sample in samples {
            if self.data.len() >= self.capacity {
                let _ = self.data.pop_front();
                self.overrun_samples = self.overrun_samples.saturating_add(1);
            }
            self.data.push_back(sample);
        }
    }

    /// Pops up to `wanted` samples into `out` and records underruns when empty.
    pub fn pop_samples(&mut self, wanted: usize, out: &mut Vec<f32>) {
        out.clear();
        out.reserve(wanted);

        for _ in 0..wanted {
            if let Some(sample) = self.data.pop_front() {
                out.push(sample);
            } else {
                self.underrun_samples = self.underrun_samples.saturating_add(1);
                break;
            }
        }
    }

    /// Returns a copied sample at `index` without consuming it.
    pub fn peek(&self, index: usize) -> Option<f32> {
        self.data.get(index).copied()
    }

    /// Removes one sample from the front if available.
    pub fn pop_front(&mut self) {
        let _ = self.data.pop_front();
    }

    /// Returns how many samples were dropped due to overflow.
    pub const fn overrun_samples(&self) -> u64 {
        self.overrun_samples
    }

    /// Returns how many sample reads encountered underrun.
    pub const fn underrun_samples(&self) -> u64 {
        self.underrun_samples
    }
}
