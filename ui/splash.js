/**
 * DeepSeek Harness 像素风启动画面。
 * 全屏单画布：深空底 + 5x7 像素字体品牌名，逐像素迪斯科式跳色闪动。
 * 加载状态（正在检查运行环境… 等）显示在品牌名下方，见 status.html 的 status-area。
 * 画面跟启动进度天然绑定：Rust 侧 window.navigate 切走状态页时动画结束，
 * 最短播放时长由 orchestrator.rs 的 MIN_SPLASH 兜底。
 */
(function () {
  'use strict';

  // 迪斯科色板：纯红色系，深红到亮红
  var PALETTE = [
    [255, 45, 45],
    [224, 24, 48],
    [196, 16, 60],
    [255, 92, 92],
    [158, 12, 40]
  ];

  var TEXT_CELL = 12;    // 品牌名像素格子边长（CSS px）
  var GAP = 2;           // 像素之间的缝隙，留出网格感

  // 5x7 像素字体：逐像素精确定义，只含品牌名用到的字母，1 表示实心像素
  var FONT = {
    A: ['01110', '10001', '10001', '11111', '10001', '10001', '10001'],
    D: ['11110', '10001', '10001', '10001', '10001', '10001', '11110'],
    E: ['11111', '10000', '10000', '11110', '10000', '10000', '11111'],
    H: ['10001', '10001', '10001', '11111', '10001', '10001', '10001'],
    K: ['10001', '10010', '10100', '11000', '10100', '10010', '10001'],
    N: ['10001', '11001', '10101', '10101', '10011', '10001', '10001'],
    P: ['11110', '10001', '10001', '11110', '10000', '10000', '10000'],
    R: ['11110', '10001', '10001', '11110', '10100', '10010', '10001'],
    S: ['01111', '10000', '10000', '01110', '00001', '00001', '11110']
  };
  var BRAND = 'DEEPSEEK HARNESS';

  // 确定性伪随机：同一 seed 永远得到同一结果，画面稳定可复现
  function hash(n) {
    var x = Math.sin(n) * 43758.5453;
    return x - Math.floor(x);
  }

  // 构造一个闪动像素：格子坐标 + 各自的跳色节拍、明暗相位
  function makePixel(gx, gy, cell, layer, gap) {
    var seed = gx * 157.31 + gy * 311.77 + layer * 71.3;
    return {
      gx: gx,
      gy: gy,
      size: cell - (gap || GAP),
      seed: seed,
      period: 0.45 + hash(seed) * 1.1,           // 换色节拍 0.45~1.55 秒
      beatPhase: hash(seed + 1.7) * 8,           // 换色相位错开
      tw: 1.6 + hash(seed + 3.1) * 3.2,          // 明暗脉动角速度
      twPhase: hash(seed + 5.3) * Math.PI * 2,   // 明暗相位错开
      base: 0.55 + hash(seed + 7.9) * 0.45       // 基准亮度
    };
  }

  // 算某像素此刻的颜色和透明度：按节拍跳色板色 + 正弦明暗脉动
  function shade(p, t, dim) {
    var step = Math.floor(t / p.period + p.beatPhase);
    var rgb = PALETTE[Math.floor(hash(p.seed + step * 91.7) * PALETTE.length)];

    var pulse = 0.5 + 0.5 * Math.sin(t * p.tw + p.twPhase);

    return { rgb: rgb, a: p.base * (0.35 + 0.65 * pulse) * dim };
  }

  // 每 2.2 秒一道竖直亮带从左扫到右，经过的像素提亮，制造扫描科技感
  function sweepBoost(x, w, t) {
    var cycle = (t % 2.2) / 2.2;
    var bandX = cycle * w * 1.3 - w * 0.15;
    var d = Math.abs(x - bandX) / (w * 0.09);

    return d < 1 ? (1 - d) * 0.9 : 0;
  }

  // 画一个像素方块，透明度过低直接跳过
  function paint(ctx, x, y, size, s, boost) {
    var a = s.a * boost;
    if (a <= 0.01) {
      return;
    }
    if (a > 1) {
      a = 1;
    }
    ctx.fillStyle = 'rgba(' + s.rgb[0] + ',' + s.rgb[1] + ',' + s.rgb[2] + ',' + a.toFixed(3) + ')';
    ctx.fillRect(x, y, size, size);
  }

  // 用 5x7 像素字体拼品牌名：字母间空 1 列、空格空 3 列，输出实心像素的格子坐标
  function brandCells() {
    var cells = [];
    var x = 0;
    for (var i = 0; i < BRAND.length; i++) {
      var ch = BRAND.charAt(i);
      if (ch === ' ') {
        x += 3;
        continue;
      }
      var glyph = FONT[ch];
      for (var gy = 0; gy < glyph.length; gy++) {
        for (var gx = 0; gx < glyph[gy].length; gx++) {
          if (glyph[gy].charAt(gx) === '1') {
            cells.push({ gx: x + gx, gy: gy });
          }
        }
      }
      x += 6;
    }

    return { cells: cells, cols: x - 1, rows: 7 };
  }

  // 画布状态：尺寸按 devicePixelRatio 放大，坐标统一用 CSS px
  function makeState(canvas) {
    return { canvas: canvas, ctx: canvas.getContext('2d'), w: 0, h: 0, bg: null, text: [] };
  }

  // 适配窗口尺寸并重置背景渐变缓存
  function fit(state) {
    var dpr = Math.min(window.devicePixelRatio || 1, 2);
    state.w = window.innerWidth;
    state.h = window.innerHeight;
    state.canvas.width = Math.round(state.w * dpr);
    state.canvas.height = Math.round(state.h * dpr);
    state.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    state.bg = null;
  }

  // 品牌名排布在窗口正中偏上，给下方状态区留位；窄窗口等比缩小保证完整可见
  function layout(state, brandMap) {
    var minWH = Math.min(state.w, state.h);
    var scale = Math.min(1, state.w * 0.88 / (brandMap.cols * TEXT_CELL));
    var cell = TEXT_CELL * scale;
    var gap = Math.max(1, GAP * scale);
    var textW = brandMap.cols * cell;
    var textH = brandMap.rows * cell;
    var ox = Math.round((state.w - textW) / 2);
    var oy = Math.round((state.h - textH) / 2 - minWH * 0.08);

    state.text = brandMap.cells.map(function (c) {
      var p = makePixel(c.gx, c.gy, cell, 1, gap);
      p.tx = ox + c.gx * cell;
      p.ty = oy + c.gy * cell;
      // 不做位移飞行，只错峰亮起，像像素屏上电
      p.sx = p.tx;
      p.sy = p.ty;
      p.delay = hash(p.seed + 17.3) * 0.35;
      p.dur = 0.3;

      return p;
    });
  }

  // 画品牌名像素：错峰淡入后原地闪动，扫描亮带经过时提亮
  function paintBrand(ctx, pixels, w, t, dim) {
    for (var i = 0; i < pixels.length; i++) {
      var p = pixels[i];
      var k = (t - p.delay) / p.dur;
      if (k <= 0) {
        continue;
      }
      if (k > 1) {
        k = 1;
      }
      var e = 1 - Math.pow(1 - k, 3);

      paint(ctx, p.tx, p.ty, p.size, shade(p, t, dim), e * (1 + sweepBoost(p.tx, w, t)));
    }
  }

  // 画一帧：深空渐变底 + 品牌名闪动
  function draw(state, t) {
    var ctx = state.ctx;
    if (!state.bg) {
      var g = ctx.createLinearGradient(0, 0, state.w, state.h);
      g.addColorStop(0, '#0d1226');
      g.addColorStop(0.55, '#101736');
      g.addColorStop(1, '#0a0e1e');
      state.bg = g;
    }
    ctx.fillStyle = state.bg;
    ctx.fillRect(0, 0, state.w, state.h);

    paintBrand(ctx, state.text, state.w, t, 1);
  }

  // 动画入口：适配窗口后跑 rAF 主循环，直到页面被切走
  function boot() {
    var canvas = document.getElementById('fx');
    if (!canvas) {
      return;
    }

    var brandMap = brandCells();
    var state = makeState(canvas);
    var reduced = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    // 尺寸变化时重排：适配画布、重排品牌名
    function refitAll() {
      fit(state);
      layout(state, brandMap);
      if (reduced) {
        draw(state, 1.2);
      }
    }

    refitAll();
    window.addEventListener('resize', refitAll);

    // 减弱动效偏好：只画一帧静态画面
    if (reduced) {
      return;
    }

    var t0 = performance.now();

    function frame(now) {
      draw(state, (now - t0) / 1000);
      requestAnimationFrame(frame);
    }

    requestAnimationFrame(frame);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
