(function () {
  var theme = 'dark';

  try {
    if (window.localStorage.getItem('coldstart.theme') === 'light') {
      theme = 'light';
    }
  } catch (_) {}

  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
})();
