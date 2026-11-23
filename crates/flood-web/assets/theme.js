(function () {
  var p = new URLSearchParams(location.search).get('theme');
  var dark = p ? p === 'dark' : window.matchMedia('(prefers-color-scheme: dark)').matches;
  document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light');
})();
