def f():
    try:
        conn = connect()
    except OSError:
        conn.close()
