fn f(x: i32) -> i32 {
    if true {
        return x;
    }
    if false {
        return x + 1;
    }
    x
}
