const fs = require('fs');
const { chromium } = require('playwright');

(async () => {
  const whale = fs.readFileSync('/tmp/whale_path.txt', 'utf8').trim();
  const S = 1024;
  const TILE = 824;            // macOS 图标网格：底托 824，四周留 100 透明边距
  const TILE_XY = (S - TILE) / 2;
  const TILE_RX = 185;
  const WHALE_W = 510; // 鲸鱼在图标里的目标宽度

  const svg =
    '<svg xmlns="http://www.w3.org/2000/svg" width="' + S + '" height="' + S + '" viewBox="0 0 ' + S + ' ' + S + '">' +
      '<defs>' +
        '<linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">' +
          '<stop offset="0" stop-color="#1d2547"></stop>' +
          '<stop offset="1" stop-color="#090c18"></stop>' +
        '</linearGradient>' +
      '</defs>' +
      '<rect x="' + TILE_XY + '" y="' + TILE_XY + '" width="' + TILE + '" height="' + TILE + '" rx="' + TILE_RX + '" fill="url(#bg)"></rect>' +
      '<rect x="' + (TILE_XY + 1.5) + '" y="' + (TILE_XY + 1.5) + '" width="' + (TILE - 3) + '" height="' + (TILE - 3) + '" rx="' + (TILE_RX - 1.5) + '" fill="none" stroke="rgba(255,255,255,0.09)" stroke-width="3"></rect>' +
      '<g id="whaleG"><path id="whale" d="' + whale + '" fill="#ffffff"></path></g>' +
    '</svg>';

  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: S, height: S } });
  await page.setContent('<!DOCTYPE html><html><head><style>*{margin:0;padding:0}body{background:transparent}</style></head><body>' + svg + '</body></html>');

  // 量出鲸鱼原始包围盒，算 transform 让它以 WHALE_W 宽度精确居中
  await page.evaluate(function (targetW) {
    var p = document.getElementById('whale');
    var b = p.getBBox();
    var s = targetW / b.width;
    var tx = 512 - (b.x + b.width / 2) * s;
    var ty = 512 - (b.y + b.height / 2) * s;
    document.getElementById('whaleG').setAttribute('transform', 'translate(' + tx + ',' + ty + ') scale(' + s + ')');
    return b;
  }, WHALE_W);

  await page.screenshot({ path: '/tmp/icon-master.png', omitBackground: true, clip: { x: 0, y: 0, width: S, height: S } });

  // 拼一张多尺寸预览图：灰底上排 256/128/64/32/16
  const sizes = [256, 128, 64, 32, 16];
  const pad = 40;
  const sheetW = sizes.reduce(function (a, b) { return a + b; }, 0) + pad * (sizes.length + 1);
  const sheetH = 256 + pad * 2 + 30;
  const page2 = await browser.newPage({ viewport: { width: sheetW, height: sheetH } });
  const masterBuf = fs.readFileSync('/tmp/icon-master.png');
  const dataUrl = 'data:image/png;base64,' + masterBuf.toString('base64');
  let cells = '';
  let x = pad;
  for (const sz of sizes) {
    const y = pad + (256 - sz) / 2;
    cells += '<div style="position:absolute;left:' + x + 'px;top:' + y + 'px;width:' + sz + 'px;height:' + sz + 'px">' +
      '<img src="' + dataUrl + '" width="' + sz + '" height="' + sz + '">' +
      '<div style="position:absolute;top:' + (sz + 6) + 'px;width:100%;text-align:center;font:12px sans-serif;color:#ddd">' + sz + '</div>' +
      '</div>';
    x += sz + pad;
  }
  await page2.setContent('<!DOCTYPE html><html><head><style>body{margin:0;background:#5a5f6b}</style></head><body>' + cells + '</body></html>');
  await page2.screenshot({ path: '/tmp/icon-preview.png' });

  await browser.close();
  console.log('done: /tmp/icon-master.png /tmp/icon-preview.png');
})().catch(function (e) { console.error(e); process.exit(1); });
