// Arma `latest.json`, el archivo que la app consulta para saber si hay una versión nueva.
// Uso: node latest-json.mjs <etiqueta vX.Y.Z> <carpeta del bundle nsis> [archivo de notas]
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [etiqueta, carpeta, archivoNotas] = process.argv.slice(2);
if (!etiqueta || !carpeta) {
  console.error('Uso: node latest-json.mjs <vX.Y.Z> <carpeta nsis> [notas]');
  process.exit(1);
}
const version = etiqueta.replace(/^v/, '');
const conf = JSON.parse(readFileSync(new URL('../tauri.conf.json', import.meta.url), 'utf8'));
if (conf.version !== version) {
  console.error(`La etiqueta (${version}) no coincide con la versión de tauri.conf.json (${conf.version}).`);
  process.exit(1);
}
const instalador = readdirSync(carpeta).find((f) => f.endsWith('-setup.exe'));
if (!instalador) {
  console.error(`No hay instalador en ${carpeta}`);
  process.exit(1);
}
const firma = readFileSync(join(carpeta, `${instalador}.sig`), 'utf8').trim();
const repo = process.env.GITHUB_REPOSITORY ?? 'MiguelMeza17/BranchBoard';
const latest = {
  version,
  notes: archivoNotas ? readFileSync(archivoNotas, 'utf8').trim() : `BranchBoard ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature: firma,
      url: `https://github.com/${repo}/releases/download/${etiqueta}/${encodeURIComponent(instalador)}`,
    },
  },
};
writeFileSync(join(carpeta, 'latest.json'), JSON.stringify(latest, null, 2) + '\n');
console.log(`latest.json listo para ${instalador}`);
