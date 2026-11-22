// The player's finished hands, kept in IndexedDB: one record per hand, so a
// new hand is one small write rather than the whole history again, and two
// tabs adding hands at once do not overwrite each other. If IndexedDB is not
// available (some private windows), the hands live only as long as the page.

const NAME = 'poker';
const HANDS = 'hands';

const request = (req) => new Promise((resolve, reject) => {
  req.onsuccess = () => resolve(req.result);
  req.onerror = () => reject(req.error);
});

const finished = (tx) => new Promise((resolve, reject) => {
  tx.oncomplete = () => resolve();
  tx.onerror = () => reject(tx.error);
  tx.onabort = () => reject(tx.error ?? new Error('the write was abandoned'));
});

const opened = (async () => {
  try {
    const open = indexedDB.open(NAME, 1);
    // Keys count up, so reading them in order gives the hands oldest first.
    open.onupgradeneeded = () => open.result.createObjectStore(HANDS, { autoIncrement: true });
    return await request(open);
  } catch {
    return null;
  }
})();

let memory = [];

/** Whether hands are kept across reloads. */
export async function lasting() {
  return (await opened) !== null;
}

/** Every stored hand, oldest first. */
export async function all() {
  const db = await opened;
  if (!db) return [...memory];
  return request(db.transaction(HANDS).objectStore(HANDS).getAll());
}

/** Adds `entries` after the stored hands, then drops the oldest beyond `keep`. */
export async function add(entries, keep) {
  const db = await opened;
  if (!db) {
    memory.push(...entries);
    memory.splice(0, Math.max(0, memory.length - keep));
    return;
  }
  const tx = db.transaction(HANDS, 'readwrite');
  const hands = tx.objectStore(HANDS);
  for (const entry of entries) hands.add(entry);
  hands.count().onsuccess = (event) => {
    let excess = event.target.result - keep;
    if (excess <= 0) return;
    hands.openCursor().onsuccess = (e) => {
      const cursor = e.target.result;
      if (!cursor || excess-- <= 0) return;
      cursor.delete();
      cursor.continue();
    };
  };
  await finished(tx);
}

/** Deletes every stored hand. */
export async function clear() {
  memory = [];
  const db = await opened;
  if (!db) return;
  const tx = db.transaction(HANDS, 'readwrite');
  tx.objectStore(HANDS).clear();
  await finished(tx);
}
