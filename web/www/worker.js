// The engine runs here, off the page's thread: pricing a decision takes long
// enough to make the table stutter otherwise. Commands in, replies out, in order.
import init, { WebTable } from './pkg/poker_web.js';

const table = init().then(() => new WebTable());
table.then(
  () => postMessage({ ready: true }),
  (error) => postMessage({ fatal: String(error) }),
);

onmessage = async (event) => {
  const { id, cmd } = event.data;
  try {
    const reply = JSON.parse((await table).command(JSON.stringify(cmd)));
    postMessage({ id, reply });
  } catch (error) {
    postMessage({ id, reply: { error: String(error) }, crashed: true });
  }
};
