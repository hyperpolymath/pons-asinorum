function load(): void {
  try {
    risky();
  } catch (e) {
    // intentional: best-effort load, caller checks the result
  }
}

function loadAndLog(): void {
  try {
    risky();
  } catch (e) {
    console.error(e);
  }
}
