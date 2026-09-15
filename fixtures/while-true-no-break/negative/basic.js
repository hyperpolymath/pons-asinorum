function f(x) {
  while (true) {
    if (x) {
      break;
    }
    doThing();
  }
}

function g() {
  while (true) {
    throw new Error("bail");
  }
}
