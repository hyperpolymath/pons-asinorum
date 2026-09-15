fn f(x: bool) {
    while x {
        do_thing();
    }
}

fn g(x: bool) {
    loop {
        if x {
            break;
        }
    }
}
