def load():
    try:
        risky()
    except Exception:
        pass  # intentional: best-effort load, caller checks the result


def load_and_log():
    try:
        risky()
    except Exception:
        log.exception("risky() failed")
