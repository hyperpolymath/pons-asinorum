fn f(x: i32) -> i32 {
    if x > 0 {
        return x;
    }
    let mut n = x;
    while true {
        n -= 1;
        if n == 0 {
            break;
        }
    }
    n
}
