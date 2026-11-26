// Adds an explicit "Edit this page on GitHub" link at the bottom of each page (#1355).
(function () {
  var btn = document.querySelector('a[href*="/edit/main/"]');
  var main = document.querySelector("#content main");
  if (!btn || !main) return;
  var a = document.createElement("a");
  a.className = "sp-edit-link";
  a.href = btn.href;
  a.rel = "noopener";
  a.textContent = "✎ Edit this page on GitHub";
  main.appendChild(a);
})();
