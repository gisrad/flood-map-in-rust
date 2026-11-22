// Minimal ES module bridge between Rust/Leptos WASM and local MapLibre GL v5 canvas.

let map = null;
let currentTheme = 'light';
let terrain3d = false;
let onSelectCb = null;
let userMarker = null;
let pendingData = {
  rivers: emptyFC(),
  stations: emptyFC(),
  robs: emptyFC(),
  reports: emptyFC(),
  selectedId: ''
};

function emptyFC() {
  return { type: 'FeatureCollection', features: [] };
}

function styleUrl(theme) {
  return theme === 'dark'
    ? 'https://tiles.openfreemap.org/styles/dark'
    : 'https://tiles.openfreemap.org/styles/positron';
}

function getPadding() {
  const mobile = window.matchMedia('(max-width: 959px)').matches;
  if (mobile) {
    return { top: 18, right: 12, bottom: 175, left: 12 };
  }
  return { top: 24, right: 420, bottom: 24, left: 20 };
}

function attachHydroLayers() {
  if (!map || !map.isStyleLoaded()) return;

  if (!map.getSource('dem')) {
    map.addSource('dem', {
      type: 'raster-dem',
      tiles: ['https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png'],
      tileSize: 256,
      encoding: 'terrarium',
      maxzoom: 14
    });
  }
  if (!map.getLayer('hillshade')) {
    map.addLayer({
      id: 'hillshade',
      type: 'hillshade',
      source: 'dem',
      layout: { visibility: terrain3d ? 'visible' : 'none' },
      paint: {
        'hillshade-exaggeration': 0.45,
        'hillshade-shadow-color': currentTheme === 'dark' ? '#040a12' : '#334155'
      }
    });
  }
  map.setTerrain(terrain3d ? { source: 'dem', exaggeration: 1.55 } : null);

  const ensureSrc = (id, data) => {
    if (map.getSource(id)) {
      map.getSource(id).setData(data);
    } else {
      map.addSource(id, { type: 'geojson', data });
    }
  };

  ensureSrc('rivers', pendingData.rivers);
  ensureSrc('stations', pendingData.stations);
  ensureSrc('robs', pendingData.robs);
  ensureSrc('reports', pendingData.reports);

  if (!map.getLayer('rivers-casing')) {
    map.addLayer({
      id: 'rivers-casing',
      type: 'line',
      source: 'rivers',
      layout: { 'line-cap': 'round', 'line-join': 'round' },
      paint: {
        'line-color': currentTheme === 'dark' ? '#07111b' : '#ffffff',
        'line-width': ['+', 3, ['*', ['min', ['coalesce', ['get', 'ratio'], 1], 3], 1.6]],
        'line-opacity': 0.72
      }
    });
  }

  if (!map.getLayer('rivers-line')) {
    map.addLayer({
      id: 'rivers-line',
      type: 'line',
      source: 'rivers',
      layout: { 'line-cap': 'round', 'line-join': 'round' },
      paint: {
        'line-color': [
          'match', ['get', 'alert_rank'],
          3, '#dc2626',
          2, '#ea580c',
          1, '#b45309',
          '#0e7490'
        ],
        'line-width': ['+', 2, ['*', ['min', ['coalesce', ['get', 'ratio'], 1], 3], 1.4]],
        'line-opacity': 0.9
      }
    });
  }

  if (!map.getLayer('stations-halo')) {
    map.addLayer({
      id: 'stations-halo',
      type: 'circle',
      source: 'stations',
      filter: ['>=', ['get', 'alert_rank'], 1],
      paint: {
        'circle-radius': ['match', ['get', 'alert_rank'], 3, 21, 2, 17, 14],
        'circle-color': [
          'match', ['get', 'alert_rank'],
          3, 'rgba(220, 38, 38, 0.24)',
          2, 'rgba(234, 88, 12, 0.22)',
          'rgba(180, 83, 9, 0.18)'
        ],
        'circle-stroke-width': 1.2,
        'circle-stroke-color': [
          'match', ['get', 'alert_rank'],
          3, '#dc2626',
          2, '#ea580c',
          '#b45309'
        ]
      }
    });
  }

  if (!map.getLayer('stations-circle')) {
    map.addLayer({
      id: 'stations-circle',
      type: 'circle',
      source: 'stations',
      paint: {
        'circle-radius': ['match', ['get', 'alert_rank'], 3, 9.5, 2, 8.5, 1, 7.5, 6.2],
        'circle-color': [
          'match', ['get', 'alert_rank'],
          3, '#dc2626',
          2, '#ea580c',
          1, '#d97706',
          ['match', ['get', 'mechanism'], 'pluvial', '#0284c7', '#0f766e']
        ],
        'circle-stroke-width': 2,
        'circle-stroke-color': currentTheme === 'dark' ? '#0b141f' : '#ffffff'
      }
    });
  }

  if (!map.getLayer('robs-circle')) {
    map.addLayer({
      id: 'robs-circle',
      type: 'circle',
      source: 'robs',
      paint: {
        'circle-radius': ['match', ['get', 'alert_rank'], 3, 9, 2, 8, 1, 7, 6],
        'circle-color': [
          'match', ['get', 'alert_rank'],
          3, '#7c3aed',
          2, '#4f46e5',
          1, '#6366f1',
          '#4338ca'
        ],
        'circle-stroke-width': 2.2,
        'circle-stroke-color': '#e0e7ff'
      }
    });
  }

  if (!map.getLayer('reports-circle')) {
    map.addLayer({
      id: 'reports-circle',
      type: 'circle',
      source: 'reports',
      paint: {
        'circle-radius': 4.8,
        'circle-color': '#e11d48',
        'circle-stroke-width': 1.5,
        'circle-stroke-color': '#ffffff',
        'circle-opacity': 0.9
      }
    });
  }

  if (!map.getLayer('selected-ring')) {
    map.addLayer({
      id: 'selected-ring',
      type: 'circle',
      source: 'stations',
      filter: ['==', ['get', 'id'], pendingData.selectedId || ''],
      paint: {
        'circle-radius': 15,
        'circle-color': 'rgba(14, 116, 144, 0.08)',
        'circle-stroke-width': 2.5,
        'circle-stroke-color': currentTheme === 'dark' ? '#38bdf8' : '#0f1f2e'
      }
    });
  }
}

export function initMap(containerId, theme, onSelect) {
  currentTheme = theme === 'dark' ? 'dark' : 'light';
  onSelectCb = onSelect;
  if (map) return;

  const ml = window.maplibregl;
  if (!ml) return;

  const mobile = window.matchMedia('(max-width: 959px)').matches;
  map = new ml.Map({
    container: containerId,
    style: styleUrl(currentTheme),
    bounds: mobile ? [[95.2, -9.8], [131.5, 4.2]] : [[94.8, -11.2], [141.2, 6.2]],
    fitBoundsOptions: { padding: getPadding(), maxZoom: 5.5 },
    maxBounds: [[75.0, -40.0], [160.0, 24.0]],
    maxPitch: 68,
    attributionControl: { compact: true, customAttribution: 'Hidrosentinel · GloFAS v4 · Open-Meteo · BMKG · PetaBencana' }
  });

  map.addControl(new ml.NavigationControl({ visualizePitch: true }), 'bottom-left');
  map.addControl(new ml.ScaleControl({ unit: 'metric' }), 'bottom-right');

  const wireClicks = () => {
    attachHydroLayers();
    for (const layerId of ['stations-circle', 'robs-circle']) {
      map.on('click', layerId, (e) => {
        const feat = e.features && e.features[0];
        if (feat && feat.properties && feat.properties.id && onSelectCb) {
          onSelectCb(String(feat.properties.id));
        }
      });
      map.on('mouseenter', layerId, () => { map.getCanvas().style.cursor = 'pointer'; });
      map.on('mouseleave', layerId, () => { map.getCanvas().style.cursor = ''; });
    }
    map.on('click', 'reports-circle', (e) => {
      const feat = e.features && e.features[0];
      if (!feat) return;
      const p = feat.properties || {};
      const coords = feat.geometry.coordinates;
      const box = document.createElement('div');
      box.className = 'map-pop';
      const title = document.createElement('strong');
      title.textContent = p.title || 'Laporan Banjir Warga';
      const meta = document.createElement('div');
      meta.textContent = p.meta || '';
      const body = document.createElement('p');
      body.textContent = p.text || '';
      box.append(title, meta, body);
      new ml.Popup({ closeButton: true, maxWidth: '260px' })
        .setLngLat(coords)
        .setDOMContent(box)
        .addTo(map);
    });
  };

  map.on('load', wireClicks);
  map.on('style.load', attachHydroLayers);
}

export function updateMapData(riversStr, stationsStr, robsStr, reportsStr, selectedId) {
  try {
    pendingData = {
      rivers: JSON.parse(riversStr),
      stations: JSON.parse(stationsStr),
      robs: JSON.parse(robsStr),
      reports: JSON.parse(reportsStr),
      selectedId: selectedId || ''
    };
  } catch {
    return;
  }
  if (!map || !map.isStyleLoaded()) return;
  for (const k of ['rivers', 'stations', 'robs', 'reports']) {
    const src = map.getSource(k);
    if (src) src.setData(pendingData[k]);
  }
  if (map.getLayer('selected-ring')) {
    map.setFilter('selected-ring', ['==', ['get', 'id'], pendingData.selectedId]);
  }
}

export function flyToPoint(lon, lat, zoom) {
  if (!map) return;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  map.flyTo({
    center: [lon, lat],
    zoom: zoom || 8.4,
    padding: getPadding(),
    duration: reduced ? 0 : 750
  });
}

export function resetIndonesiaView() {
  if (!map) return;
  const mobile = window.matchMedia('(max-width: 959px)').matches;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  map.fitBounds(mobile ? [[95.2, -9.8], [131.5, 4.2]] : [[94.8, -11.2], [141.2, 6.2]], {
    padding: getPadding(),
    maxZoom: 5.5,
    duration: reduced ? 0 : 700
  });
}

export function setMapTheme(theme) {
  const next = theme === 'dark' ? 'dark' : 'light';
  if (next === currentTheme) return;
  currentTheme = next;
  if (map) map.setStyle(styleUrl(next));
}

export function toggle3DTerrain(enabled) {
  terrain3d = !!enabled;
  if (!map || !map.isStyleLoaded()) return;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const dur = reduced ? 0 : 800;
  if (terrain3d) {
    if (map.getSource('dem')) map.setTerrain({ source: 'dem', exaggeration: 1.55 });
    if (map.getLayer('hillshade')) map.setLayoutProperty('hillshade', 'visibility', 'visible');
    map.easeTo({ pitch: 52, duration: dur });
  } else {
    map.setTerrain(null);
    if (map.getLayer('hillshade')) map.setLayoutProperty('hillshade', 'visibility', 'none');
    map.easeTo({ pitch: 0, duration: dur });
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
  if (userMarker) {
    userMarker.remove();
  }
}
