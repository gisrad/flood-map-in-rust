// Jembatan ES module antara Rust/Leptos (WASM) dan MapLibre GL v5 lokal.
// Bentuk ikon = mekanisme, warna = level indikasi model.

const BASE_STYLE_URL = 'https://tiles.openfreemap.org/styles/positron';
const DEM_TILES = 'https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png';
const RIVERS_URL = '/data/rivers.geojson';
const INDONESIA = [[94.0, -11.5], [141.5, 6.5]];
const LEVEL_KEYS = ['unknown', 'normal', 'high', 'very_high', 'extreme'];

const THEMES = {
  light: {
    land: '#F5F3EE',
    sea: '#DCE7EC',
    river: '#2F7FC1',
    ink: '#0E1C24',
    inkSoft: '#4A5A63',
    line: '#D9D5CC',
    road: '#E4DFD6',
    green: '#ECEBE3',
    halo: 'rgba(245,243,238,0.92)',
    seaLabel: '#4D7188',
    shadow: '#5B5346',
    highlight: '#FFFFFF',
    sky: '#DCE7EC'
  },
  dark: {
    land: '#122530',
    sea: '#0B1820',
    river: '#5DB4E8',
    ink: '#E8EEF1',
    inkSoft: '#9FB0B8',
    line: '#2A3F4B',
    road: '#1D3340',
    green: '#14293A',
    halo: 'rgba(11,24,32,0.9)',
    seaLabel: '#7FA3B8',
    shadow: '#02080C',
    highlight: '#3B5868',
    sky: '#0B1820'
  }
};

const LEVEL_COLORS = {
  unknown: '#B8C0C4',
  normal: '#7C8F99',
  high: '#E0A100',
  very_high: '#D4471F',
  extreme: '#7B2A8C'
};

let map = null;
let baseStyle = null;
let currentTheme = 'light';
let terrain3d = false;
let onSelectCb = null;
let userMarker = null;
let hoverPopup = null;
let riversRaw = null;
let riversRequested = false;
let data = {
  stations: emptyFC(),
  robs: emptyFC(),
  reports: emptyFC(),
  selectedId: ''
};

function emptyFC() {
  return { type: 'FeatureCollection', features: [] };
}

function reducedMotion() {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

function isMobile() {
  return window.matchMedia('(max-width: 959px)').matches;
}

function getPadding() {
  if (isMobile()) {
    const sheet = document.querySelector('.inspector');
    const h = window.innerHeight;
    let bottom = 196;
    if (sheet && sheet.classList.contains('snap-half')) bottom = Math.round(h * 0.56) + 16;
    else if (sheet && sheet.classList.contains('snap-full')) bottom = Math.round(h * 0.56) + 16;
    return { top: 64, right: 16, bottom: Math.min(bottom, h - 160), left: 16 };
  }
  return { top: 72, right: 440, bottom: 40, left: 40 };
}

// ---------- Gaya peta dasar ----------

const LABEL_FIELD = ['coalesce', ['get', 'name:id'], ['get', 'name:latin'], ['get', 'name_en'], ['get', 'name']];

function transformStyle(src, themeName) {
  const t = THEMES[themeName];
  const style = JSON.parse(JSON.stringify(src));
  const drop = new Set([
    'highway-shield-non-us', 'highway-shield-us-interstate', 'road_shield_us',
    'aeroway-taxiway', 'aeroway-runway-casing', 'aeroway-area', 'aeroway-runway', 'airport',
    'railway_transit_dashline', 'railway_service_dashline', 'railway_dashline', 'highway-name-path'
  ]);
  style.layers = style.layers.filter((l) => !drop.has(l.id));
  for (const l of style.layers) {
    const sl = l['source-layer'] || '';
    l.paint = l.paint || {};
    l.layout = l.layout || {};
    if (l.type === 'background') {
      l.paint['background-color'] = t.land;
    } else if (l.type === 'fill' && sl === 'water') {
      l.paint['fill-color'] = t.sea;
    } else if (l.type === 'line' && sl === 'waterway') {
      l.paint['line-color'] = t.river;
      l.paint['line-opacity'] = ['interpolate', ['linear'], ['zoom'], 4, 0.35, 8, 0.55, 12, 0.75];
      l.paint['line-width'] = ['interpolate', ['exponential', 1.4], ['zoom'], 4, 0.3, 8, 0.8, 12, 2, 16, 5];
    } else if (l.type === 'fill' && (sl === 'park' || sl === 'landcover' || sl === 'landuse')) {
      l.paint['fill-color'] = t.green;
    } else if (l.type === 'fill' && sl === 'building') {
      l.paint['fill-color'] = t.road;
      l.paint['fill-outline-color'] = t.line;
    } else if (l.type === 'fill' && sl === 'transportation') {
      l.paint['fill-color'] = t.land;
    } else if (l.type === 'line' && sl === 'transportation') {
      l.paint['line-color'] = /casing/.test(l.id) ? t.line : t.road;
      l.paint['line-opacity'] = ['interpolate', ['linear'], ['zoom'], 5, 0.25, 9, 0.6, 13, 0.9];
    } else if (l.type === 'line' && sl === 'boundary') {
      l.paint['line-color'] = t.inkSoft;
      l.paint['line-opacity'] = l.id === 'boundary_2' ? 0.4 : 0.22;
    } else if (l.type === 'symbol') {
      if (l.layout['text-field']) l.layout['text-field'] = LABEL_FIELD;
      const isWater = sl === 'water_name' || sl === 'waterway';
      l.paint['text-color'] = isWater ? t.seaLabel : t.inkSoft;
      l.paint['text-halo-color'] = t.halo;
      l.paint['text-halo-width'] = 1.2;
      l.paint['text-halo-blur'] = 0.4;
      if (/^label_country/.test(l.id)) {
        l.paint['text-color'] = t.ink;
        l.paint['text-opacity'] = ['case', ['==', ['get', 'iso_a2'], 'ID'], 0.9, 0.38];
        l.layout['text-transform'] = 'uppercase';
        l.layout['text-letter-spacing'] = 0.12;
        l.layout['text-size'] = ['interpolate', ['linear'], ['zoom'], 2, 9, 5, 12];
      } else if (sl === 'place') {
        l.paint['text-opacity'] = ['interpolate', ['linear'], ['zoom'], 4, 0.7, 7, 0.95];
        if (l.id === 'label_city_capital') l.paint['text-color'] = t.ink;
      }
    }
  }
  return style;
}

async function loadBaseStyle() {
  if (baseStyle) return baseStyle;
  const resp = await fetch(BASE_STYLE_URL);
  baseStyle = await resp.json();
  return baseStyle;
}

// ---------- Ikon (canvas → addImage) ----------

function shapePath(ctx, shape, c, r) {
  ctx.beginPath();
  if (shape === 'circle') {
    ctx.arc(c, c, r, 0, Math.PI * 2);
  } else if (shape === 'square') {
    const s = r * 0.9;
    const k = s * 0.38;
    ctx.moveTo(c - s + k, c - s);
    ctx.arcTo(c + s, c - s, c + s, c + s, k);
    ctx.arcTo(c + s, c + s, c - s, c + s, k);
    ctx.arcTo(c - s, c + s, c - s, c - s, k);
    ctx.arcTo(c - s, c - s, c + s, c - s, k);
  } else if (shape === 'diamond') {
    const s = r * 1.18;
    ctx.moveTo(c, c - s);
    ctx.lineTo(c + s, c);
    ctx.lineTo(c, c + s);
    ctx.lineTo(c - s, c);
  } else {
    // tetes: ujung di atas, badan bulat di bawah
    const rr = r * 0.78;
    const cy = c + r * 0.22;
    ctx.moveTo(c, cy - rr * 2.05);
    ctx.bezierCurveTo(c + rr * 0.35, cy - rr * 1.35, c + rr, cy - rr * 0.75, c + rr, cy);
    ctx.arc(c, cy, rr, 0, Math.PI);
    ctx.bezierCurveTo(c - rr, cy - rr * 0.75, c - rr * 0.35, cy - rr * 1.35, c, cy - rr * 2.05);
  }
  ctx.closePath();
}

function makeIcon(shape, level, theme) {
  const ratio = 2;
  const size = 26 * ratio;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d');
  const c = size / 2;
  const r = shape === 'drop' ? 7 * ratio : 8 * ratio;
  // Level tidak hanya dikodekan warna: cincin luar makin tebal untuk Tinggi, Sangat tinggi, Ekstrem.
  const ringSteps = { high: 1, very_high: 2, extreme: 3 }[level] || 0;
  if (ringSteps > 0) {
    shapePath(ctx, shape, c, r);
    ctx.lineJoin = 'round';
    ctx.lineWidth = (2 + 1.2 * ringSteps) * ratio;
    ctx.strokeStyle = theme === 'dark' ? '#E8EEF1' : '#0E1C24';
    ctx.stroke();
  }
  shapePath(ctx, shape, c, r);
  ctx.fillStyle = LEVEL_COLORS[level];
  ctx.fill();
  if (level === 'unknown') {
    ctx.save();
    shapePath(ctx, shape, c, r);
    ctx.clip();
    ctx.strokeStyle = 'rgba(14,28,36,0.45)';
    ctx.lineWidth = 1.4 * ratio;
    for (let x = -size; x < size * 2; x += 4.5 * ratio) {
      ctx.beginPath();
      ctx.moveTo(x, size);
      ctx.lineTo(x + size, 0);
      ctx.stroke();
    }
    ctx.restore();
  }
  shapePath(ctx, shape, c, r);
  ctx.lineWidth = 2 * ratio;
  ctx.strokeStyle = theme === 'dark' ? '#0B1820' : '#FFFFFF';
  ctx.stroke();
  return { image: ctx.getImageData(0, 0, size, size), ratio };
}

function addIcons() {
  for (const shape of ['circle', 'square', 'diamond', 'drop']) {
    for (const level of LEVEL_KEYS) {
      const id = `ico-${shape}-${level}`;
      if (map.hasImage(id)) map.removeImage(id);
      const { image, ratio } = makeIcon(shape, level, currentTheme);
      map.addImage(id, image, { pixelRatio: ratio });
    }
  }
}

// ---------- Sungai OSM (milik worker aset) ----------
// Geometri berasal dari OpenStreetMap, berarah ke hilir. Tidak ada garis buatan:
// bila berkas belum ada, tidak ada garis sungai data yang digambar.

const DETAIL_ZOOM = 9;
const detailCache = new Map(); // station_id -> FeatureCollection | null (gagal) | 'loading'

function isFC(gj) {
  return gj && gj.type === 'FeatureCollection' && Array.isArray(gj.features);
}

function requestRivers() {
  if (riversRequested) return;
  riversRequested = true;
  fetch(RIVERS_URL, { headers: { accept: 'application/geo+json, application/json' } })
    .then((r) => (r.ok ? r.json() : null))
    .then((gj) => {
      if (isFC(gj)) {
        riversRaw = gj;
        pushRivers();
      }
    })
    .catch(() => {});
}

function requestDetail(stationId) {
  const sid = String(stationId || '');
  if (!/^[A-Za-z0-9_-]+$/.test(sid) || detailCache.has(sid)) return;
  detailCache.set(sid, 'loading');
  fetch(`/data/rivers/${sid}.geojson`, { headers: { accept: 'application/geo+json, application/json' } })
    .then((r) => (r.ok ? r.json() : null))
    .then((gj) => {
      detailCache.set(sid, isFC(gj) ? gj : null);
      if (isFC(gj)) pushRivers();
    })
    .catch(() => detailCache.set(sid, null));
}

function requestVisibleDetails() {
  if (!map || map.getZoom() < DETAIL_ZOOM) return;
  const b = map.getBounds();
  let n = 0;
  for (const f of data.stations.features) {
    const [lon, lat] = f.geometry.coordinates;
    if (b.contains([lon, lat]) && !detailCache.has(String(f.properties.id))) {
      requestDetail(f.properties.id);
      if (++n >= 6) break;
    }
  }
}

function useDetail(sid, zoom) {
  const d = detailCache.get(sid);
  return isFC(d) && (zoom >= DETAIL_ZOOM - 1 || sid === data.selectedId);
}

function riversJoined() {
  const levelById = new Map();
  for (const f of data.stations.features) {
    levelById.set(String(f.properties.id), f.properties.river_level);
  }
  const zoom = map ? map.getZoom() : 0;
  const out = [];
  const pushFeat = (f, sidFallback) => {
    const p = f.properties || {};
    const sid = String(p.station_id || sidFallback || '');
    out.push({
      type: 'Feature',
      geometry: f.geometry,
      properties: {
        station_id: sid,
        name: p.name || '',
        level: levelById.get(sid) || 'none',
        selected: sid !== '' && sid === data.selectedId
      }
    });
  };
  if (riversRaw) {
    for (const f of riversRaw.features) {
      const sid = String((f.properties && f.properties.station_id) || '');
      if (!useDetail(sid, zoom)) pushFeat(f, sid);
    }
  }
  for (const [sid, fc] of detailCache) {
    if (useDetail(sid, zoom)) for (const f of fc.features) pushFeat(f, sid);
  }
  return { type: 'FeatureCollection', features: out };
}

let lastDetailMode = null;
function onMoveEnd() {
  requestVisibleDetails();
  const mode = map.getZoom() >= DETAIL_ZOOM - 1;
  if (mode !== lastDetailMode) {
    lastDetailMode = mode;
    pushRivers();
  }
}

function pushRivers() {
  if (!map || !map.getSource('rivers')) return;
  map.getSource('rivers').setData(riversJoined());
  updateAnimationNeed();
}

// ---------- Lapisan data ----------

const ICON_SIZE = ['match', ['get', 'level'], 'extreme', 1.2, 'very_high', 1.08, 'high', 0.98, 'normal', 0.82, 0.78];
const LEVEL_RANK = ['match', ['get', 'level'], 'extreme', 4, 'very_high', 3, 'high', 2, 'normal', 1, 0];

function levelColorExpr(fallback) {
  return ['match', ['get', 'level'],
    'high', LEVEL_COLORS.high,
    'very_high', LEVEL_COLORS.very_high,
    'extreme', LEVEL_COLORS.extreme,
    fallback];
}

function attachDataLayers() {
  if (!map) return;
  const style = map.getStyle();
  if (!style || !style.glyphs) return; // gaya sementara sebelum gaya dasar termuat
  const t = THEMES[currentTheme];
  addIcons();

  if (!map.getSource('dem')) {
    map.addSource('dem', { type: 'raster-dem', tiles: [DEM_TILES], tileSize: 256, encoding: 'terrarium', maxzoom: 12 });
    map.addSource('dem-hs', { type: 'raster-dem', tiles: [DEM_TILES], tileSize: 256, encoding: 'terrarium', maxzoom: 12 });
  }
  if (!map.getLayer('hillshade')) {
    const firstSymbol = map.getStyle().layers.find((l) => l.type === 'symbol');
    map.addLayer({
      id: 'hillshade',
      type: 'hillshade',
      source: 'dem-hs',
      layout: { visibility: terrain3d ? 'visible' : 'none' },
      paint: {
        'hillshade-exaggeration': 0.5,
        'hillshade-shadow-color': t.shadow,
        'hillshade-highlight-color': t.highlight,
        'hillshade-accent-color': t.shadow
      }
    }, firstSymbol ? firstSymbol.id : undefined);
  }

  const ensure = (id, d) => {
    if (map.getSource(id)) map.getSource(id).setData(d);
    else map.addSource(id, { type: 'geojson', data: d });
  };
  ensure('rivers', riversJoined());
  ensure('stations', data.stations);
  ensure('robs', data.robs);
  ensure('reports', data.reports);

  const add = (layer) => { if (!map.getLayer(layer.id)) map.addLayer(layer); };

  add({
    id: 'rivers-sel-casing', type: 'line', source: 'rivers',
    filter: ['==', ['get', 'selected'], true],
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: {
      'line-color': t.ink,
      'line-opacity': 0.55,
      'line-width': ['interpolate', ['linear'], ['zoom'], 4, 4.5, 8, 7, 12, 11]
    }
  });
  add({
    id: 'rivers-base', type: 'line', source: 'rivers',
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: {
      'line-color': levelColorExpr(t.river),
      'line-opacity': ['match', ['get', 'level'], 'high', 0.95, 'very_high', 0.95, 'extreme', 0.95, 0.8],
      'line-width': ['interpolate', ['linear'], ['zoom'],
        4, ['match', ['get', 'level'], 'extreme', 3.2, 'very_high', 2.8, 'high', 2.4, 1.4],
        8, ['match', ['get', 'level'], 'extreme', 5.5, 'very_high', 4.8, 'high', 4.2, 2.4],
        12, ['match', ['get', 'level'], 'extreme', 9, 'very_high', 8, 'high', 7, 4]]
    }
  });
  add({
    id: 'rivers-flow', type: 'line', source: 'rivers',
    filter: ['match', ['get', 'level'], ['high', 'very_high', 'extreme'], true, false],
    layout: { 'line-cap': 'butt', 'line-join': 'round' },
    paint: {
      'line-color': currentTheme === 'dark' ? 'rgba(232,238,241,0.55)' : 'rgba(255,255,255,0.7)',
      'line-width': ['interpolate', ['linear'], ['zoom'], 4, 1, 8, 1.6, 12, 2.6],
      'line-dasharray': [0, 4, 3]
    }
  });

  add({
    id: 'pulse', type: 'circle', source: 'stations',
    filter: ['==', ['get', 'level'], 'extreme'],
    paint: {
      'circle-radius': 14,
      'circle-color': 'rgba(0,0,0,0)',
      'circle-stroke-color': LEVEL_COLORS.extreme,
      'circle-stroke-width': 2,
      'circle-stroke-opacity': 0.0,
      'circle-pitch-alignment': 'map'
    }
  });

  add({
    id: 'reports-icon', type: 'symbol', source: 'reports',
    layout: {
      'icon-image': 'ico-drop-very_high',
      'icon-size': ['interpolate', ['linear'], ['zoom'], 4, 0.62, 9, 0.8],
      'icon-allow-overlap': true,
      'icon-anchor': 'bottom'
    }
  });

  const pointLayout = (shapeExpr) => ({
    'icon-image': ['concat', 'ico-', shapeExpr, '-', ['get', 'level']],
    'icon-size': ICON_SIZE,
    'icon-allow-overlap': true,
    'icon-ignore-placement': true,
    'symbol-sort-key': LEVEL_RANK,
    'text-field': ['step', ['zoom'], '', 6.2, ['get', 'name']],
    'text-font': ['Noto Sans Regular'],
    'text-size': 11.5,
    'text-offset': [0, 1.25],
    'text-anchor': 'top',
    'text-optional': true,
    'text-max-width': 9
  });
  const pointPaint = {
    'text-color': t.ink,
    'text-halo-color': t.halo,
    'text-halo-width': 1.4
  };

  add({
    id: 'robs-icon', type: 'symbol', source: 'robs',
    layout: pointLayout('diamond'),
    paint: pointPaint
  });
  add({
    id: 'stations-icon', type: 'symbol', source: 'stations',
    layout: pointLayout(['match', ['get', 'mech'], 'pluvial', 'square', 'circle']),
    paint: pointPaint
  });

  for (const src of ['stations', 'robs']) {
    add({
      id: `${src}-selected`, type: 'circle', source: src,
      filter: ['==', ['get', 'id'], data.selectedId || '__none__'],
      paint: {
        'circle-radius': 15,
        'circle-color': 'rgba(0,0,0,0)',
        'circle-stroke-width': 2.4,
        'circle-stroke-color': t.ink
      }
    });
  }

  if (terrain3d) {
    map.setTerrain({ source: 'dem', exaggeration: 1.5 });
    applySky();
  }
  updateAnimationNeed();
}

function applySky() {
  if (!map || typeof map.setSky !== 'function') return;
  const t = THEMES[currentTheme];
  map.setSky({
    'sky-color': t.sky,
    'horizon-color': t.land,
    'fog-color': t.land,
    'sky-horizon-blend': 0.6,
    'horizon-fog-blend': 0.6,
    'fog-ground-blend': 0.4
  });
}

// ---------- Animasi (hanya bila menyampaikan keadaan) ----------

// Tiap langkah menggeser pola dash +0,5 satuan dari awal ke akhir garis.
// Garis OSM berarah ke hilir, sehingga aliran tampak bergerak ke hilir.
const DASH_SEQ = [
  [0, 4, 3], [0.5, 4, 2.5], [1, 4, 2], [1.5, 4, 1.5], [2, 4, 1], [2.5, 4, 0.5], [3, 4, 0],
  [0, 0.5, 3, 3.5], [0, 1, 3, 3], [0, 1.5, 3, 2.5], [0, 2, 3, 2], [0, 2.5, 3, 1.5], [0, 3, 3, 1], [0, 3.5, 3, 0.5]
];
let animFrame = null;
let animNeeded = false;
let lastStep = -1;

function updateAnimationNeed() {
  const hasRivers = !!riversRaw || [...detailCache.values()].some(isFC);
  const flowing = hasRivers && data.stations.features.some((f) => ['high', 'very_high', 'extreme'].includes(f.properties.river_level));
  const pulsing = data.stations.features.some((f) => f.properties.level === 'extreme');
  animNeeded = flowing || pulsing;
  syncAnimation();
}

function syncAnimation() {
  const run = animNeeded && !reducedMotion() && document.visibilityState === 'visible' && map;
  if (run && animFrame === null) {
    animFrame = requestAnimationFrame(tick);
  } else if (!run && animFrame !== null) {
    cancelAnimationFrame(animFrame);
    animFrame = null;
  }
  if (!run && map && map.getLayer('pulse')) {
    map.setPaintProperty('pulse', 'circle-stroke-opacity', 0);
  }
}

function tick(ts) {
  animFrame = null;
  if (!map) return;
  const step = Math.floor(ts / 70) % DASH_SEQ.length;
  if (step !== lastStep && map.getLayer('rivers-flow')) {
    lastStep = step;
    map.setPaintProperty('rivers-flow', 'line-dasharray', DASH_SEQ[step]);
  }
  if (map.getLayer('pulse')) {
    const phase = (ts % 2200) / 2200;
    map.setPaintProperty('pulse', 'circle-radius', 11 + phase * 14);
    map.setPaintProperty('pulse', 'circle-stroke-opacity', 0.7 * (1 - phase));
  }
  syncAnimation();
}

document.addEventListener('visibilitychange', syncAnimation);
window.matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', syncAnimation);

// ---------- API untuk Rust ----------

function popupBox(title, meta, body) {
  const box = document.createElement('div');
  box.className = 'map-pop';
  const h = document.createElement('strong');
  h.textContent = title;
  box.append(h);
  if (meta) {
    const m = document.createElement('div');
    m.className = 'map-pop-meta';
    m.textContent = meta;
    box.append(m);
  }
  if (body) {
    const p = document.createElement('p');
    p.textContent = body;
    box.append(p);
  }
  return box;
}

function wireInteractions(ml) {
  for (const layerId of ['stations-icon', 'robs-icon']) {
    map.on('click', layerId, (e) => {
      const f = e.features && e.features[0];
      if (f && f.properties && f.properties.id && onSelectCb) onSelectCb(String(f.properties.id));
    });
    map.on('mousemove', layerId, (e) => {
      const f = e.features && e.features[0];
      if (!f) return;
      map.getCanvas().style.cursor = 'pointer';
      const p = f.properties || {};
      if (!hoverPopup) {
        hoverPopup = new ml.Popup({ closeButton: false, closeOnClick: false, offset: 14, className: 'hover-pop' });
      }
      hoverPopup.setLngLat(f.geometry.coordinates).setDOMContent(popupBox(p.name || '', p.level_label || '', '')).addTo(map);
    });
    map.on('mouseleave', layerId, () => {
      map.getCanvas().style.cursor = '';
      if (hoverPopup) hoverPopup.remove();
    });
  }
  map.on('click', 'reports-icon', (e) => {
    const f = e.features && e.features[0];
    if (!f) return;
    const p = f.properties || {};
    new ml.Popup({ closeButton: true, maxWidth: '280px', offset: 12 })
      .setLngLat(f.geometry.coordinates)
      .setDOMContent(popupBox(p.title || 'Laporan banjir warga', p.meta || '', p.text || ''))
      .addTo(map);
  });
  map.on('mouseenter', 'reports-icon', () => { map.getCanvas().style.cursor = 'pointer'; });
  map.on('mouseleave', 'reports-icon', () => { map.getCanvas().style.cursor = ''; });
}

export function initMap(containerId, theme, onSelect) {
  currentTheme = theme === 'dark' ? 'dark' : 'light';
  onSelectCb = onSelect;
  if (map) return;
  const ml = window.maplibregl;
  if (!ml) return;

  map = new ml.Map({
    container: containerId,
    style: { version: 8, sources: {}, layers: [{ id: 'bg', type: 'background', paint: { 'background-color': THEMES[currentTheme].land } }] },
    bounds: INDONESIA,
    fitBoundsOptions: { padding: getPadding() },
    maxBounds: [[40.0, -60.0], [200.0, 60.0]],
    maxPitch: 70,
    attributionControl: {
      compact: true,
      customAttribution: 'GloFAS v4 via Open-Meteo · BMKG · PetaBencana'
    }
  });
  map.addControl(new ml.NavigationControl({ visualizePitch: true }), 'bottom-right');
  map.addControl(new ml.ScaleControl({ unit: 'metric', maxWidth: 90 }), 'bottom-right');

  map.on('style.load', attachDataLayers);
  map.on('moveend', onMoveEnd);
  map.once('load', () => {
    if (isMobile()) {
      const el = map.getContainer().querySelector('.maplibregl-ctrl-attrib');
      if (el) el.classList.remove('maplibregl-compact-show');
    }
  });
  wireInteractions(ml);
  requestRivers();
  loadBaseStyle()
    .then((s) => map.setStyle(transformStyle(s, currentTheme), { diff: false }))
    .catch(() => {});
}

export function updateMapData(stationsStr, robsStr, reportsStr, selectedId) {
  try {
    data = {
      stations: JSON.parse(stationsStr),
      robs: JSON.parse(robsStr),
      reports: JSON.parse(reportsStr),
      selectedId: selectedId || ''
    };
  } catch {
    return;
  }
  if (!map) return;
  for (const k of ['stations', 'robs', 'reports']) {
    const src = map.getSource(k);
    if (src) src.setData(data[k]);
  }
  for (const src of ['stations', 'robs']) {
    if (map.getLayer(`${src}-selected`)) {
      map.setFilter(`${src}-selected`, ['==', ['get', 'id'], data.selectedId || '__none__']);
    }
  }
  if (data.selectedId) requestDetail(data.selectedId);
  requestVisibleDetails();
  pushRivers();
  updateAnimationNeed();
}

export function flyToPoint(lon, lat, zoom) {
  if (!map) return;
  map.flyTo({ center: [lon, lat], zoom: zoom || 8.4, padding: getPadding(), duration: reducedMotion() ? 0 : 800, essential: true });
}

export function resetIndonesiaView() {
  if (!map) return;
  map.fitBounds(INDONESIA, {
    padding: getPadding(),
    pitch: terrain3d ? 45 : 0,
    bearing: 0,
    duration: reducedMotion() ? 0 : 700
  });
}

export function setMapTheme(theme) {
  const next = theme === 'dark' ? 'dark' : 'light';
  if (next === currentTheme) return;
  currentTheme = next;
  if (!map || !baseStyle) return;
  if (hoverPopup) hoverPopup.remove();
  map.setStyle(transformStyle(baseStyle, next), { diff: false });
}

export function toggle3DTerrain(enabled) {
  const next = !!enabled;
  if (next === terrain3d) return;
  terrain3d = next;
  if (!map || !map.getSource('dem')) return;
  const dur = reducedMotion() ? 0 : 900;
  if (terrain3d) {
    map.setTerrain({ source: 'dem', exaggeration: 1.5 });
    applySky();
    if (map.getLayer('hillshade')) map.setLayoutProperty('hillshade', 'visibility', 'visible');
    map.easeTo({ pitch: 50, duration: dur });
  } else {
    map.setTerrain(null);
    if (map.getLayer('hillshade')) map.setLayoutProperty('hillshade', 'visibility', 'none');
    map.easeTo({ pitch: 0, bearing: 0, duration: dur });
  }
}

export function setUserLocation(lon, lat) {
  if (!map || !window.maplibregl) return;
  if (!userMarker) {
    const el = document.createElement('div');
    el.className = 'user-gps-pin';
    el.title = 'Posisi Anda';
    userMarker = new window.maplibregl.Marker({ element: el, anchor: 'center' });
  }
  userMarker.setLngLat([lon, lat]).addTo(map);
}

export function clearUserLocation() {
  if (userMarker) userMarker.remove();
}

export function resizeMap() {
  if (map) map.resize();
}
