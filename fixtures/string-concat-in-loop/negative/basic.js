function sumNumbers(items) {
  let n = 0;
  for (const x of items) {
    n += x;
  }
  return n;
}

function buildOnce(items) {
  let s = "";
  s += "prefix";
  for (const x of items) {
    doThing(x);
  }
  return s;
}

function buildInNestedFunction(items) {
  for (const x of items) {
    const render = () => {
      let t = "";
      t += "z";
      return t;
    };
    render();
  }
}
