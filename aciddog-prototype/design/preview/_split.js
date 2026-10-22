/* Forge preview card — clones <template id="card-content"> into a dark and a
   light pane so reviewers see both themes side by side. */
(function () {
  function build() {
    const tpl = document.getElementById('card-content');
    if (!tpl) return;
    const split = document.createElement('div');
    split.className = 'split';

    for (const theme of ['dark', 'light']) {
      const pane = document.createElement('div');
      pane.className = `pane ${theme}`;
      pane.setAttribute('data-theme', theme);
      pane.dataset.label = theme;
      pane.appendChild(tpl.content.cloneNode(true));
      split.appendChild(pane);
    }
    document.body.innerHTML = '';
    document.body.appendChild(split);
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', build);
  } else {
    build();
  }
})();
