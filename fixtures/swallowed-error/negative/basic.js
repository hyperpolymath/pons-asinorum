function load() {
  try {
    risky();
  } catch (e) {
    // intentional: best-effort load, caller checks the result
  }
}

function loadAndLog() {
  try {
    risky();
  } catch (e) {
    console.error(e);
  }
}
