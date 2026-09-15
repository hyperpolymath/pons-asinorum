fn f(x: bool) {
    loop {
        if x {
            break;
        }
        do_thing();
    }
}

fn g() {
    while false {
        do_thing();
    }
}
