function f(x) {
  while (x) {
    doThing();
  }
}

function g(y) {
  for (const x of y) {
    doThing(x);
  }
}
