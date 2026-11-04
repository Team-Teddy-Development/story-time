// Minimal orchestration saga. Each step: { name, execute(ctx), compensate?(ctx, result) }.
// On the first failing step, completed steps are compensated in reverse order.
// A failing compensation is recorded and does not stop the remaining ones,
// so the caller always learns exactly what could not be rolled back.

export async function runSaga(steps, ctx = {}) {
  const done = [];
  const results = {};
  for (const step of steps) {
    try {
      const result = await step.execute(ctx, results);
      results[step.name] = result;
      done.push({ step, result });
    } catch (error) {
      const { compensated, compensationFailures } = await compensate(done, ctx);
      return { ok: false, failedStep: step.name, error, results, compensated, compensationFailures };
    }
  }
  return { ok: true, results, compensated: [], compensationFailures: [] };
}

async function compensate(done, ctx) {
  const compensated = [];
  const compensationFailures = [];
  for (const { step, result } of [...done].reverse()) {
    if (typeof step.compensate !== 'function') continue;
    try {
      await step.compensate(ctx, result);
      compensated.push(step.name);
    } catch (error) {
      compensationFailures.push({ step: step.name, error });
    }
  }
  return { compensated, compensationFailures };
}
