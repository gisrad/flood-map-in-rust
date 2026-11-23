import { test, expect } from '@playwright/test';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { MARKER } from '../fixtures/snapshot.mjs';
import { DIST } from '../fixtures/server.mjs';


test('test fixture marker and test modules never enter production dist', async () => {
  const marker = Buffer.from(MARKER);
  const leaked = [];
  const visit = async (directory) => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (/playwright|\.spec\.mjs|snapshot\.mjs|server\.mjs/i.test(entry.name)
        || (await readFile(path)).includes(marker)) leaked.push(path);
    }
  };
  await visit(DIST);
  expect(leaked, 'Production dist must contain only production assets, never synthetic API fixtures').toEqual([]);
});
