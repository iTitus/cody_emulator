//! Queue primitives used by the audio runtime.
//!
//! These types intentionally avoid blocking and instead track overrun/underrun
//! counters so real-time audio paths can degrade gracefully.

use crossbeam_queue::ArrayQueue;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Trait for a thread-safe event queue with fixed capacity and overrun tracking.
pub trait EventQueue<T>: Send + Sync {
    fn capacity(&self) -> usize;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn push_drop_oldest(&self, value: T);
    fn pop_front(&self) -> Option<T>;
    fn drain_into(&self, out: &mut Vec<T>);
    fn overrun_count(&self) -> u64;
}

/// Trait for a thread-safe PCM sample buffer with fixed capacity and overrun/underrun tracking.
pub trait PcmBuffer: Send + Sync {
    fn len(&self) -> usize;
    fn capacity(&self) -> usize;
    fn push_samples(&self, samples: &[f32]);
    fn pop_samples(&self, wanted: usize, out: &mut Vec<f32>);
    fn pop_front(&self);
    fn overrun_samples(&self) -> u64;
    fn underrun_samples(&self) -> u64;
}

/// Shared handle types for event queues and PCM buffers.
pub type EventQueueHandle<T> = Arc<dyn EventQueue<T>>;
/// Shared handle type for PCM buffers.
pub type PcmBufferHandle = Arc<dyn PcmBuffer>;

/// Factory methods for creating event queues and PCM buffers with specified capacities.
pub fn new_event_queue<T: Send + Sync + 'static>(capacity: usize) -> EventQueueHandle<T> {
    Arc::new(LockFreeQueue::with_capacity(capacity))
}

/// Creates a real PCM buffer with the given capacity for actual audio processing.
pub fn new_pcm_buffer(capacity: usize) -> PcmBufferHandle {
    Arc::new(LockFreePcmRingBuffer::with_capacity(capacity))
}

/// Creates a dummy PCM buffer that discards writes and always reads as empty, for testing or diagnostics.
pub fn new_dummy_pcm_buffer(capacity: usize) -> PcmBufferHandle {
    Arc::new(DummyPcmBuffer::with_capacity(capacity))
}

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

impl<T: Send + Sync> EventQueue<T> for LockFreeQueue<T> {
    fn capacity(&self) -> usize {
        LockFreeQueue::capacity(self)
    }

    fn len(&self) -> usize {
        LockFreeQueue::len(self)
    }

    fn is_empty(&self) -> bool {
        LockFreeQueue::is_empty(self)
    }

    fn push_drop_oldest(&self, value: T) {
        LockFreeQueue::push_drop_oldest(self, value);
    }

    fn pop_front(&self) -> Option<T> {
        LockFreeQueue::pop_front(self)
    }

    fn drain_into(&self, out: &mut Vec<T>) {
        LockFreeQueue::drain_into(self, out);
    }

    fn overrun_count(&self) -> u64 {
        LockFreeQueue::overrun_count(self)
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

impl PcmBuffer for LockFreePcmRingBuffer {
    fn len(&self) -> usize {
        LockFreePcmRingBuffer::len(self)
    }

    fn capacity(&self) -> usize {
        LockFreePcmRingBuffer::capacity(self)
    }

    fn push_samples(&self, samples: &[f32]) {
        LockFreePcmRingBuffer::push_samples(self, samples);
    }

    fn pop_samples(&self, wanted: usize, out: &mut Vec<f32>) {
        LockFreePcmRingBuffer::pop_samples(self, wanted, out);
    }

    fn pop_front(&self) {
        LockFreePcmRingBuffer::pop_front(self);
    }

    fn overrun_samples(&self) -> u64 {
        LockFreePcmRingBuffer::overrun_samples(self)
    }

    fn underrun_samples(&self) -> u64 {
        LockFreePcmRingBuffer::underrun_samples(self)
    }
}

/// A dummy PCM buffer that discards writes and always reads as empty.
#[derive(Clone)]
pub struct DummyPcmBuffer {
    capacity: usize,
    overrun_samples: Arc<AtomicU64>,
    underrun_samples: Arc<AtomicU64>,
}

impl fmt::Debug for DummyPcmBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DummyPcmBuffer")
            .field("len", &0usize)
            .field("capacity", &self.capacity)
            .field("overrun_samples", &self.overrun_samples())
            .field("underrun_samples", &self.underrun_samples())
            .finish()
    }
}

impl DummyPcmBuffer {
    /// Creates a dummy PCM buffer with a logical capacity for diagnostics.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        Self {
            capacity,
            overrun_samples: Arc::new(AtomicU64::new(0)),
            underrun_samples: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl PcmBuffer for DummyPcmBuffer {
    fn len(&self) -> usize {
        0
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn push_samples(&self, _samples: &[f32]) {}

    fn pop_samples(&self, wanted: usize, out: &mut Vec<f32>) {
        out.clear();
        if wanted > 0 {
            self.underrun_samples
                .fetch_add(wanted as u64, Ordering::Relaxed);
        }
    }

    fn pop_front(&self) {
        self.underrun_samples.fetch_add(1, Ordering::Relaxed);
    }

    fn overrun_samples(&self) -> u64 {
        self.overrun_samples.load(Ordering::Relaxed)
    }

    fn underrun_samples(&self) -> u64 {
        self.underrun_samples.load(Ordering::Relaxed)
    }
}
