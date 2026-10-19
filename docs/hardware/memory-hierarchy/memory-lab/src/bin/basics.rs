fn sum(values: &[u64]) -> u64 {
    let mut total = 0;
    for &value in values {
        total += value;
    }
    total
}

fn main() {
    let values: Vec<u64> = vec![10, 20, 30, 40];
    println!("sum = {}", sum(&values));
    println!("one u64 = {} bytes", std::mem::size_of::<u64>());
    println!("element buffer = {} bytes", values.len() * 8);
    for (index, value) in values.iter().enumerate() {
        println!("index {index}: value {value}, address {value:p}");
    }
}
