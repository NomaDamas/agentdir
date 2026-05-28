import test, { type ExecutionContext } from 'ava'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { SnapshotWorkspace, Workspace } from '../index.js'

function createTmpDir() {
  return realpathSync(mkdtempSync(join(tmpdir(), 'agentdir-test-')))
}

function createSourceDir() {
  const dir = createTmpDir()
  writeFileSync(join(dir, 'file1.txt'), 'hello')
  writeFileSync(join(dir, 'file2.txt'), 'world')
  mkdirSync(join(dir, 'subdir'))
  writeFileSync(join(dir, 'subdir', 'nested.txt'), 'nested content')
  return dir
}

function cleanDir(dir: string) {
  rmSync(dir, { recursive: true, force: true })
}

function createWorkspace() {
  const dir = createTmpDir()
  const ws = Workspace.init(dir)
  return { dir, ws }
}

async function expectRejects(t: ExecutionContext, promise: Promise<unknown>) {
  await t.throwsAsync(promise)
}

test('init / open: init creates workspace', (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir)
    t.truthy(ws)
    t.true(existsSync(dir))
  } finally {
    cleanDir(dir)
  }
})

test('init / open: init with reflink strategy', (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'reflink')
    t.truthy(ws)
    t.true(existsSync(dir))
  } finally {
    cleanDir(dir)
  }
})

test('init / open: init with symlink strategy', (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'symlink')
    t.truthy(ws)
    t.true(existsSync(dir))
  } finally {
    cleanDir(dir)
  }
})

test('init / open: init with hardlink strategy', (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'hardlink')
    t.truthy(ws)
    t.true(existsSync(dir))
  } finally {
    cleanDir(dir)
  }
})

test('init / open: init with virtual strategy', (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'virtual')
    t.truthy(ws)
    t.true(existsSync(dir))
  } finally {
    cleanDir(dir)
  }
})

test('init / open: init with invalid strategy throws', (t) => {
  const dir = createTmpDir()
  try {
    t.throws(() => Workspace.init(dir, 'invalid'))
  } finally {
    cleanDir(dir)
  }
})

test('init / open: open existing workspace works', async (t) => {
  const dir = createTmpDir()
  try {
    Workspace.init(dir)
    const ws = Workspace.open(dir)
    const status = await ws.status()
    t.is(status.totalEntries, 0)
  } finally {
    cleanDir(dir)
  }
})

test('init / open: open non-existent path throws', (t) => {
  const dir = join(createTmpDir(), 'missing')
  const parent = dir.replace(/\/missing$/, '')
  try {
    t.throws(() => Workspace.open(dir))
  } finally {
    cleanDir(parent)
  }
})

test('init / open: init then open round-trip', async (t) => {
  const dir = createTmpDir()
  const source = createSourceDir()
  try {
    const ws = Workspace.init(dir)
    await ws.map(source, '/src')
    const reopened = Workspace.open(dir)
    t.true(await reopened.exists('/src/file1.txt'))
    t.is((await reopened.readBytes('/src/file1.txt')).toString(), 'hello')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: map returns summary with correct entriesAdded count', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    const summary = await ws.map(source, '/src')
    t.is(summary.entriesAdded, 5)
    t.is(summary.errors, 0)
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: map creates materialized files', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    t.true(existsSync(join(dir, 'src', 'file1.txt')))
    t.is(readFileSync(join(dir, 'src', 'file2.txt'), 'utf8'), 'world')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: map multiple sources to different mounts', async (t) => {
  const { dir, ws } = createWorkspace()
  const first = createSourceDir()
  const second = createSourceDir()
  try {
    await ws.map(first, '/first')
    await ws.map(second, '/second')
    t.true(await ws.exists('/first/file1.txt'))
    t.true(await ws.exists('/second/subdir/nested.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(first)
    cleanDir(second)
  }
})

test('map / unmap: unmap returns entriesRemoved', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const summary = await ws.unmap('/src')
    t.is(summary.entriesRemoved, 5)
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: unmap non-existent mount returns zero', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const summary = await ws.unmap('/missing')
    t.is(summary.entriesRemoved, 0)
  } finally {
    cleanDir(dir)
  }
})

test('map / unmap: map then unmap then status shows zero entries', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.unmap('/src')
    const status = await ws.status()
    t.is(status.totalEntries, 0)
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: map verifies files are accessible via readBytes', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    t.is((await ws.readBytes('/src/file1.txt')).toString(), 'hello')
    t.is((await ws.readBytes('/src/file2.txt')).toString(), 'world')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: map nested directory structure', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    t.true(await ws.exists('/src/subdir'))
    t.true(await ws.exists('/src/subdir/nested.txt'))
    t.is((await ws.readBytes('/src/subdir/nested.txt')).toString(), 'nested content')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: map summary has reflinked or copied greater than zero', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    const summary = await ws.map(source, '/src')
    t.true(summary.reflinked + summary.copied + summary.symlinked + summary.hardlinked > 0)
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('map / unmap: map with non-existent source returns zero entries', async (t) => {
  const { dir, ws } = createWorkspace()
  const missingDir = join(createTmpDir(), 'missing-source')
  try {
    const summary = await ws.map(missingDir, '/src')
    t.is(summary.entriesAdded, 0)
  } finally {
    cleanDir(dir)
  }
})

test('virtual ops: mkdir creates virtual dir', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await ws.mkdir('/docs')
    t.true(await ws.exists('/docs'))
    t.true(statSync(join(dir, 'docs')).isDirectory())
  } finally {
    cleanDir(dir)
  }
})

test('virtual ops: mkdir existing path throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await ws.mkdir('/docs')
    await expectRejects(t, ws.mkdir('/docs'))
  } finally {
    cleanDir(dir)
  }
})

test('virtual ops: rmdir removes empty dir', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await ws.mkdir('/docs')
    await ws.rmdir('/docs', false)
    t.false(await ws.exists('/docs'))
  } finally {
    cleanDir(dir)
  }
})

test('virtual ops: rmdir recursive removes children', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await ws.mkdir('/docs')
    await ws.mkdir('/docs/nested')
    await ws.rmdir('/docs', true)
    t.false(await ws.exists('/docs'))
  } finally {
    cleanDir(dir)
  }
})

test('virtual ops: rmdir non-recursive with children throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await ws.mkdir('/docs')
    await ws.mkdir('/docs/nested')
    await expectRejects(t, ws.rmdir('/docs', false))
  } finally {
    cleanDir(dir)
  }
})

test('virtual ops: mv moves file to new path', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.mv('/src/file1.txt', '/moved/file1.txt')
    t.false(await ws.exists('/src/file1.txt'))
    t.true(await ws.exists('/moved/file1.txt'))
    t.is((await ws.readBytes('/moved/file1.txt')).toString(), 'hello')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('virtual ops: mv non-existent source throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.mv('/missing.txt', '/dest.txt'))
  } finally {
    cleanDir(dir)
  }
})

test('virtual ops: cp copies entry and both exist after', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.cp('/src/file1.txt', '/copy/file1.txt')
    t.true(await ws.exists('/src/file1.txt'))
    t.true(await ws.exists('/copy/file1.txt'))
    t.is((await ws.readBytes('/copy/file1.txt')).toString(), 'hello')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('virtual ops: rename changes last component', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.rename('/src/file1.txt', 'renamed.txt')
    t.false(await ws.exists('/src/file1.txt'))
    t.true(await ws.exists('/src/renamed.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('virtual ops: rename non-existent throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.rename('/missing.txt', 'new.txt'))
  } finally {
    cleanDir(dir)
  }
})

test('query apis: exists returns true for mapped file', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    t.true(await ws.exists('/src/file1.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('query apis: exists returns false for non-existent', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    t.false(await ws.exists('/missing.txt'))
  } finally {
    cleanDir(dir)
  }
})

test('query apis: stat returns all expected fields', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const stat = await ws.stat('/src/file1.txt')
    t.is(stat.virtualPath, '/src/file1.txt')
    t.true(stat.sourcePath.endsWith('file1.txt'))
    t.is(stat.sizeBytes, 5)
    t.true(typeof stat.mtimeNs === 'number')
    t.true(typeof stat.entryType === 'string')
    t.true(typeof stat.materialized === 'boolean')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('query apis: stat entryType is File for files', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const stat = await ws.stat('/src/file1.txt')
    t.is(stat.entryType, 'File')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('query apis: stat entryType is Directory for dirs', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const stat = await ws.stat('/src/subdir')
    t.is(stat.entryType, 'Directory')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('query apis: stat non-existent throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.stat('/missing.txt'))
  } finally {
    cleanDir(dir)
  }
})

test('query apis: readBytes returns correct content', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    t.is((await ws.readBytes('/src/file2.txt')).toString(), 'world')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('query apis: readBytes non-existent throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.readBytes('/missing.txt'))
  } finally {
    cleanDir(dir)
  }
})

test('query apis: rglob matches files with wildcard', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const matches = await ws.rglob('/**/*.txt')
    t.true(matches.includes('/src/file1.txt'))
    t.true(matches.includes('/src/subdir/nested.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('query apis: rglob returns empty for non-matching pattern', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    t.deepEqual(await ws.rglob('/**/*.md'), [])
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('error handling: operations on non-existent paths throw descriptive messages', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const error = await t.throwsAsync(ws.mv('/missing.txt', '/dest.txt'))
    t.truthy(error?.message)
  } finally {
    cleanDir(dir)
  }
})

test('error handling: invalid empty path throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.stat('/'))
  } finally {
    cleanDir(dir)
  }
})

test('error handling: map to already-mapped mount throws or handles gracefully', async (t) => {
  const { dir, ws } = createWorkspace()
  const first = createSourceDir()
  const second = createSourceDir()
  try {
    await ws.map(first, '/src')
    try {
      await ws.map(second, '/src')
      t.true(await ws.exists('/src/file1.txt'))
    } catch (error) {
      t.truthy((error as Error).message)
    }
  } finally {
    cleanDir(dir)
    cleanDir(first)
    cleanDir(second)
  }
})

test('error handling: rmdir non-empty non-recursive throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await ws.mkdir('/parent')
    await ws.mkdir('/parent/child')
    await expectRejects(t, ws.rmdir('/parent', false))
  } finally {
    cleanDir(dir)
  }
})

test('error handling: mv to existing path behavior', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    try {
      await ws.mv('/src/file1.txt', '/src/file2.txt')
      t.true(await ws.exists('/src/file2.txt'))
    } catch (error) {
      t.truthy((error as Error).message)
    }
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('error handling: stat on non-existent throws error with path in message', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const error = await t.throwsAsync(ws.stat('/missing.txt'))
    t.true(error?.message.includes('/missing.txt'))
  } finally {
    cleanDir(dir)
  }
})

test('error handling: readBytes on directory throws', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await expectRejects(t, ws.readBytes('/src/subdir'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('error handling: unmap non-existent mount returns zero', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const summary = await ws.unmap('/missing')
    t.is(summary.entriesRemoved, 0)
  } finally {
    cleanDir(dir)
  }
})

test('error handling: destroySnapshot non-existent throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.destroySnapshot('/missing'))
  } finally {
    cleanDir(dir)
  }
})

test('error handling: init on already-initialized workspace handles gracefully', (t) => {
  const dir = createTmpDir()
  try {
    Workspace.init(dir)
    t.notThrows(() => Workspace.init(dir))
  } finally {
    cleanDir(dir)
  }
})

test('batch map: mapBatch with multiple files', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    const summary = await ws.mapBatch([
      [join(source, 'file1.txt'), '/batch/file1.txt'],
      [join(source, 'file2.txt'), '/batch/file2.txt'],
    ])
    t.is(summary.entriesAdded, 2)
    t.deepEqual(summary.errors, [])
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('batch map: mapBatch returns correct summary counts', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    const summary = await ws.mapBatch([[join(source, 'file1.txt'), '/x/file1.txt']])
    t.is(summary.entriesAdded, 1)
    t.is(typeof summary.reflinked, 'number')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('batch map: mapBatch with empty array', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const summary = await ws.mapBatch([])
    t.is(summary.entriesAdded, 0)
    t.deepEqual(summary.errors, [])
  } finally {
    cleanDir(dir)
  }
})

test('batch map: mapBatch rejects directory source', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await t.throwsAsync(ws.mapBatch([[source, '/shouldfail']]), { message: /batch map only accepts files/ })
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('batch map: mapBatch entries accessible after mapping', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.mapBatch([[join(source, 'file1.txt'), '/b/file1.txt']])
    t.is((await ws.readBytes('/b/file1.txt')).toString(), 'hello')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('batch map: mapBatch summary fields are all numbers', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    const summary = await ws.mapBatch([[join(source, 'file1.txt'), '/y/file1.txt']])
    t.is(typeof summary.entriesAdded, 'number')
    t.is(typeof summary.reflinked, 'number')
    t.is(typeof summary.copied, 'number')
    t.is(typeof summary.symlinked, 'number')
    t.is(typeof summary.hardlinked, 'number')
    t.is(typeof summary.dirsCreated, 'number')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('refresh: refresh on unchanged workspace returns zeros', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const summary = await ws.refresh()
    t.is(summary.added, 0)
    t.is(summary.refreshed, 0)
    t.is(summary.removed, 0)
    t.is(summary.errors, 0)
  } finally {
    cleanDir(dir)
  }
})

test('refresh: refresh detects added file', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    writeFileSync(join(source, 'added.txt'), 'added')
    const summary = await ws.refresh()
    t.true(summary.added >= 1)
    t.true(await ws.exists('/src/added.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('refresh: refresh detects modified file', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    writeFileSync(join(source, 'file1.txt'), 'changed')
    const summary = await ws.refresh()
    t.true(summary.refreshed >= 1)
    t.is((await ws.readBytes('/src/file1.txt')).toString(), 'changed')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('refresh: refresh detects deleted file', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    rmSync(join(source, 'file2.txt'))
    const summary = await ws.refresh()
    t.true(summary.removed >= 1)
    t.false(await ws.exists('/src/file2.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('refresh: refresh summary has correct field types', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const summary = await ws.refresh()
    t.is(typeof summary.added, 'number')
    t.is(typeof summary.refreshed, 'number')
    t.is(typeof summary.removed, 'number')
    t.is(typeof summary.errors, 'number')
  } finally {
    cleanDir(dir)
  }
})

test('refresh: refresh after map shows no changes', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const summary = await ws.refresh()
    t.is(summary.added, 0)
    t.is(summary.refreshed, 0)
    t.is(summary.removed, 0)
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('export mapping: exportMapping returns source-to-virtual by default', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const mapping = await ws.exportMapping()
    t.is(mapping[join(source, 'file1.txt')], '/src/file1.txt')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('export mapping: exportMapping with reverse true returns virtual-to-source', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const mapping = await ws.exportMapping(true)
    t.is(mapping['/src/file1.txt'], join(source, 'file1.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('export mapping: exportMapping returns Record string string', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const mapping = await ws.exportMapping()
    t.true(mapping !== null)
    t.is(typeof mapping, 'object')
    t.false(Array.isArray(mapping))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('export mapping: exportMapping with relativeTo', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const mapping = await ws.exportMapping(false, source)
    t.is(mapping['file1.txt'], '/src/file1.txt')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('export mapping: exportMapping on empty workspace returns empty', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    t.deepEqual(await ws.exportMapping(), {})
  } finally {
    cleanDir(dir)
  }
})

test('export mapping: exportMapping keys and values are strings', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const mapping = await ws.exportMapping()
    for (const [key, value] of Object.entries(mapping)) {
      t.is(typeof key, 'string')
      t.is(typeof value, 'string')
    }
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('strategy: init with reflink strategy works', async (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'reflink')
    const status = await ws.status()
    t.is(status.totalEntries, 0)
  } finally {
    cleanDir(dir)
  }
})

test('strategy: init with symlink strategy works', async (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'symlink')
    const status = await ws.status()
    t.is(status.totalEntries, 0)
  } finally {
    cleanDir(dir)
  }
})

test('strategy: init with hardlink strategy works', async (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'hardlink')
    const status = await ws.status()
    t.is(status.totalEntries, 0)
  } finally {
    cleanDir(dir)
  }
})

test('strategy: init with virtual strategy works', async (t) => {
  const dir = createTmpDir()
  try {
    const ws = Workspace.init(dir, 'virtual')
    const status = await ws.status()
    t.is(status.totalEntries, 0)
  } finally {
    cleanDir(dir)
  }
})

test('strategy: each strategy produces accessible files via readBytes', async (t) => {
  const strategies = ['reflink', 'symlink', 'hardlink', 'virtual']
  for (const strategy of strategies) {
    const dir = createTmpDir()
    const source = createSourceDir()
    try {
      const ws = Workspace.init(dir, strategy)
      await ws.map(source, '/src')
      t.is((await ws.readBytes('/src/file1.txt')).toString(), 'hello')
    } finally {
      cleanDir(dir)
      cleanDir(source)
    }
  }
})

test('strategy: unknown strategy throws descriptive error', (t) => {
  const dir = createTmpDir()
  try {
    const error = t.throws(() => Workspace.init(dir, 'unknown'))
    t.true(Boolean(error?.message))
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: listSnapshots returns empty array initially', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    t.deepEqual(await ws.listSnapshots(), [])
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: listSnapshots returns array of strings', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const snapshots = await ws.listSnapshots()
    t.true(Array.isArray(snapshots))
    for (const snapshot of snapshots) {
      t.is(typeof snapshot, 'string')
    }
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: destroySnapshot on non-existent throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.destroySnapshot('/missing'))
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: snapshot lifecycle if applicable leaves empty list', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const before = await ws.listSnapshots()
    t.deepEqual(before, [])
    await expectRejects(t, ws.destroySnapshot('/missing'))
    t.deepEqual(await ws.listSnapshots(), [])
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: listSnapshots type check', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const snapshots = await ws.listSnapshots()
    t.true(Array.isArray(snapshots))
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: destroySnapshot removes from list', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.destroySnapshot('/missing'))
    t.false((await ws.listSnapshots()).includes('missing'))
  } finally {
    cleanDir(dir)
  }
})

test('status: status returns correct fields after init', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const status = await ws.status()
    t.is(status.totalEntries, 0)
    t.is(status.sourceRoots, 0)
    t.is(typeof status.materializedRoot, 'string')
    t.is(typeof status.lastUpdatedEpochSecs, 'number')
  } finally {
    cleanDir(dir)
  }
})

test('status: status totalEntries matches expected count after map', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const status = await ws.status()
    t.is(status.totalEntries, 5)
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('status: status sourceRoots increments after map', async (t) => {
  const { dir, ws } = createWorkspace()
  const first = createSourceDir()
  const second = createSourceDir()
  try {
    await ws.map(first, '/first')
    await ws.map(second, '/second')
    const status = await ws.status()
    t.is(status.sourceRoots, 2)
  } finally {
    cleanDir(dir)
    cleanDir(first)
    cleanDir(second)
  }
})

test('status: status materializedRoot is a string path', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const status = await ws.status()
    t.is(typeof status.materializedRoot, 'string')
    t.true(status.materializedRoot.length > 0)
  } finally {
    cleanDir(dir)
  }
})

test('refresh with hash verification: unchanged workspace returns zeros', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const summary = await ws.refreshWithHashVerification(true)
    t.is(summary.added, 0)
    t.is(summary.refreshed, 0)
    t.is(summary.removed, 0)
    t.is(summary.errors, 0)
  } finally {
    cleanDir(dir)
  }
})

test('refresh with hash verification: detects modified file', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    writeFileSync(join(source, 'file1.txt'), 'hash-verified-change')
    const summary = await ws.refreshWithHashVerification(true)
    t.true(summary.refreshed >= 1)
    t.is((await ws.readBytes('/src/file1.txt')).toString(), 'hash-verified-change')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('refresh with hash verification: summary fields have correct types', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    const summary = await ws.refreshWithHashVerification(false)
    t.is(typeof summary.added, 'number')
    t.is(typeof summary.refreshed, 'number')
    t.is(typeof summary.removed, 'number')
    t.is(typeof summary.errors, 'number')
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: create snapshot returns SnapshotWorkspace', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    t.truthy(snapshot)
    t.true(snapshot instanceof SnapshotWorkspace)
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshots: created snapshot can read files from base workspace', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    t.is((await snapshot.readBytes('/src/file1.txt')).toString(), 'hello')
    t.true(await snapshot.exists('/src/subdir/nested.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshots: snapshot write creates isolated copy', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    await snapshot.write('/src/file1.txt', Buffer.from('snapshot hello'))
    t.is((await snapshot.readBytes('/src/file1.txt')).toString(), 'snapshot hello')
    t.is((await ws.readBytes('/src/file1.txt')).toString(), 'hello')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshots: snapshot appears in listSnapshots after creation', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.snapshot('mysnap')
    t.true((await ws.listSnapshots()).includes('mysnap'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshots: creating duplicate snapshot name throws', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.snapshot('mysnap')
    await expectRejects(t, ws.snapshot('mysnap'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshots: snapshot destroy removes it from disk', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    await snapshot.destroy()
    t.false((await ws.listSnapshots()).includes('mysnap'))
    t.false(existsSync(join(dir, 'snapshots', 'mysnap')))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshots: open existing snapshot returns SnapshotWorkspace', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.snapshot('mysnap')
    const snapshot = await ws.openSnapshot('mysnap')
    t.truthy(snapshot)
    t.true(snapshot instanceof SnapshotWorkspace)
    t.is((await snapshot.readBytes('/src/file2.txt')).toString(), 'world')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshots: open non-existent snapshot throws', async (t) => {
  const { dir, ws } = createWorkspace()
  try {
    await expectRejects(t, ws.openSnapshot('missing'))
  } finally {
    cleanDir(dir)
  }
})

test('snapshots: opened snapshot can read and write independently', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    await ws.snapshot('mysnap')
    const snapshot = await ws.openSnapshot('mysnap')
    await snapshot.write('/src/file2.txt', Buffer.from('snapshot world'))
    t.is((await snapshot.readBytes('/src/file2.txt')).toString(), 'snapshot world')
    t.is((await ws.readBytes('/src/file2.txt')).toString(), 'world')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshot workspace: exists returns correct boolean', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    t.true(await snapshot.exists('/src/file1.txt'))
    t.false(await snapshot.exists('/src/missing.txt'))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshot workspace: stat returns expected fields', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    const stat = await snapshot.stat('/src/file1.txt')
    t.is(stat.virtualPath, '/src/file1.txt')
    t.true(stat.sourcePath.endsWith('file1.txt'))
    t.is(stat.sizeBytes, 5)
    t.true(typeof stat.mtimeNs === 'number')
    t.true(typeof stat.entryType === 'string')
    t.true(typeof stat.materialized === 'boolean')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshot workspace: readBytes returns correct content', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    t.is((await snapshot.readBytes('/src/subdir/nested.txt')).toString(), 'nested content')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshot workspace: write creates or overwrites file in snapshot', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    await snapshot.write('/src/file1.txt', Buffer.from('first snapshot value'))
    await snapshot.write('/src/file1.txt', Buffer.from('second snapshot value'))
    t.is((await snapshot.readBytes('/src/file1.txt')).toString(), 'second snapshot value')
    t.is((await ws.readBytes('/src/file1.txt')).toString(), 'hello')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshot workspace: exportMapping returns mapping dict', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    const mapping = await snapshot.exportMapping()
    t.true(mapping !== null)
    t.is(typeof mapping, 'object')
    t.is(mapping[join(source, 'file1.txt')], '/src/file1.txt')
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})

test('snapshot workspace: destroy removes snapshot directory', async (t) => {
  const { dir, ws } = createWorkspace()
  const source = createSourceDir()
  try {
    await ws.map(source, '/src')
    const snapshot = await ws.snapshot('mysnap')
    await snapshot.destroy()
    await expectRejects(t, ws.openSnapshot('mysnap'))
    t.false(existsSync(join(dir, 'snapshots', 'mysnap')))
  } finally {
    cleanDir(dir)
    cleanDir(source)
  }
})
