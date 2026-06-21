/// Configuration processor script.
///
/// Demonstrates how a host application can use Wscript
/// to let users write custom configuration logic.

/// Process a list of threshold values and return the count
/// of values that exceed the limit.
@export
fn count_above_threshold(limit: i32) -> i32 {
    let values = [10, 25, 50, 75, 100, 150, 200];
    let mut count = 0;
    let mut i = 0;
    let len = values.len();
    while i < len {
        let v = values[i];
        if v > limit {
            count = count + 1;
        }
        i = i + 1;
    }
    return count;
}

/// Compute a weighted score from individual ratings.
@export
fn weighted_score(quality: i32, speed: i32, reliability: i32) -> i32 {
    // Weights: quality=40%, speed=25%, reliability=35%
    let score = (quality * 40 + speed * 25 + reliability * 35) / 100;
    return score;
}

/// Classify a numeric value into a category.
/// Returns: 0=low, 1=medium, 2=high, 3=critical
@export
fn classify(value: i32) -> i32 {
    return match value {
        0 => 0,
        _ => {
            if value < 25 {
                return 0;
            }
            if value < 50 {
                return 1;
            }
            if value < 75 {
                return 2;
            }
            return 3;
        },
    };
}
