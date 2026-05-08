use std::collections::VecDeque;

/// 统计嵌套队列中第n个位置元素的均值和方差
/// # 参数
/// * `nested_queues` - 外层队列，每个元素是一个内部队列
/// * `index` - 要统计的内部队列的索引位置（从0开始）
/// * `sample_variance` - 是否使用样本方差（除以n-1），否则使用总体方差（除以n）
/// # 返回
/// 成功返回(均值, 方差)，失败返回错误信息
pub fn calculate_mean_variance<T>(
    nested_queues: &[T],
    index: usize,
    sample_variance: bool,
) -> Result<(f64, f64), &'static str>
where
    T: AsRef<[f64]>,
{
    let mut values = Vec::new();
    for queue in nested_queues {
        let queue_ref = queue.as_ref();
        if queue_ref.len() > index {
            values.push(queue_ref[index]);
        }
    }

    if values.is_empty() {
        return Err("No valid elements found at the given index");
    }

    let count = values.len() as f64;
    let sum: f64 = values.iter().sum();
    let mean = sum / count;

    let squared_diff_sum: f64 = values.iter().map(|&x| (x - mean).powi(2)).sum();
    let variance = if sample_variance {
        if count <= 1.0 {
            return Err("Sample variance requires at least 2 elements");
        }
        squared_diff_sum / (count - 1.0)
    } else {
        squared_diff_sum / count
    };

    Ok((mean, variance))
}

/// VecDeque类型嵌套队列的统计版本
pub fn calculate_mean_variance_deque<T>(
    nested_queues: &[VecDeque<T>],
    index: usize,
    sample_variance: bool,
) -> Result<(f64, f64), &'static str>
where
    T: Into<f64> + Copy,
{
    let mut values = Vec::new();
    for queue in nested_queues {
        if queue.len() > index {
            values.push(queue[index].into());
        }
    }

    if values.is_empty() {
        return Err("No valid elements found at the given index");
    }

    let count = values.len() as f64;
    let sum: f64 = values.iter().sum();
    let mean = sum / count;

    let squared_diff_sum: f64 = values.iter().map(|&x| (x - mean).powi(2)).sum();
    let variance = if sample_variance {
        if count <= 1.0 {
            return Err("Sample variance requires at least 2 elements");
        }
        squared_diff_sum / (count - 1.0)
    } else {
        squared_diff_sum / count
    };

    Ok((mean, variance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn test_calculate_mean_variance() {
        let queues = vec![
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0, 6.0],
            vec![7.0, 8.0, 9.0],
            vec![10.0],
        ];

        let (mean, var) = calculate_mean_variance(&queues, 1, false).unwrap();
        assert!((mean - 5.0).abs() < 1e-9);
        assert!((var - 6.0).abs() < 1e-9);

        let (_, sample_var) = calculate_mean_variance(&queues, 1, true).unwrap();
        assert!((sample_var - 9.0).abs() < 1e-9);
    }

    #[test]
    fn test_calculate_mean_variance_deque() {
        let mut queues = Vec::new();
        let mut q1 = VecDeque::new();
        q1.push_back(1);
        q1.push_back(2);
        q1.push_back(3);
        queues.push(q1);
        let mut q2 = VecDeque::new();
        q2.push_back(4);
        q2.push_back(5);
        q2.push_back(6);
        queues.push(q2);
        let mut q3 = VecDeque::new();
        q3.push_back(7);
        q3.push_back(8);
        q3.push_back(9);
        queues.push(q3);

        let (mean, var) = calculate_mean_variance_deque(&queues, 1, false).unwrap();
        assert!((mean - 5.0).abs() < 1e-9);
        assert!((var - 6.0).abs() < 1e-9);
    }

    #[test]
    fn test_empty_input() {
        let queues: Vec<Vec<f64>> = vec![];
        assert!(calculate_mean_variance(&queues, 0, false).is_err());
    }

    #[test]
    fn test_all_short_queues() {
        let queues = vec![vec![1.0], vec![2.0], vec![3.0]];
        assert!(calculate_mean_variance(&queues, 1, false).is_err());
    }
}
