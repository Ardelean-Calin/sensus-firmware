use core::ops::{Add, Mul, Sub};

pub struct Filter<T: Copy> {
    value: Option<T>,
    alpha: f32,
}

impl<T: Copy> Default for Filter<T> {
    /// Creates a default Filter. The default behavior is to tend towards more filtering.
    ///
    /// A alpha value of 0.329 means a 4*tau (time constant) of 10*sample period.
    /// That means if we sample every 30s, a step response will get to 99% in 300s.
    /// For further tuning we can use the formula:
    /// Assume the desired time to get to 99% is `z`; With `t` we denote the sample period.
    /// Let's denote the ratio `t/z` as `r`. So for example a `r` of 0.1 means we want our filter
    /// to reach 99% in 10 times the sample period.
    ///
    /// In order to calculate alpha, denoted with `a`, we can use this formula:
    /// a = 1 - e^(-4*r)
    fn default() -> Self {
        Self {
            value: Default::default(),
            alpha: 0.329,
        }
    }
}

impl<T: Copy> Filter<T> {
    /// Creates a new filtered float with given alpha constant.
    pub fn new(alpha: f32) -> Self {
        if !(0.0..=1.0).contains(&alpha) {
            panic!(
                "Wrong alpha value of {:?}. Expected a number between 0 and 1!",
                alpha
            );
        }

        Self { value: None, alpha }
    }

    pub fn get_value(&self) -> Option<T> {
        self.value
    }

    /// Resets the filter. This way if a large break between measurements took place
    /// and we feed new data, our filter will start from new.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.value = Default::default();
    }
}

impl<T> Filter<T>
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Mul<f32, Output = T>,
{
    /// Feeds a new value to the filter, resulting in the stored value being the filtered one.
    pub fn feed(&mut self, new_value: T) -> T {
        if let Some(prev_val) = self.value {
            let filtered = prev_val + ((new_value - prev_val) * self.alpha);
            self.value = Some(filtered);
        } else {
            self.value = Some(new_value);
        }

        defmt::unwrap!(self.get_value())
    }
}
