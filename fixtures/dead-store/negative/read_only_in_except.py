def f():
    state = "start"
    try:
        state = "mid"
        step()
    except ValueError:
        log(state)
