// Mounts an animated 3D football onto any <canvas class="ball3d"> on the page.
// The ball is a high-poly sphere wrapped with a procedurally generated soccer-ball
// texture (Telstar pattern: 12 black pentagons at the vertices of an icosahedron,
// connected by white hexagons whose edges are the icosahedron edges).

function fallback(canvas, reason) {
    console.warn('[ball.js] WebGL fallback:', reason);
    const span = document.createElement('span');
    span.className = 'ball-fallback';
    span.textContent = '⚽';
    span.style.fontSize = canvas.clientWidth ? `${canvas.clientWidth * 0.9}px` : '4rem';
    span.style.display = 'inline-block';
    span.style.lineHeight = '1';
    canvas.replaceWith(span);
}

let sharedTexturePromise = null;
function getSoccerTexture(THREE) {
    if (sharedTexturePromise) return sharedTexturePromise;
    sharedTexturePromise = new Promise((resolve) => {
        const canvas = buildSoccerCanvas(512);
        const tex = new THREE.CanvasTexture(canvas);
        tex.anisotropy = 4;
        tex.needsUpdate = true;
        resolve(tex);
    });
    return sharedTexturePromise;
}

// 12 icosahedron vertices (normalized to unit sphere)
function icosahedronVertices() {
    const phi = (1 + Math.sqrt(5)) / 2;
    const norm = 1 / Math.sqrt(1 + phi * phi);
    const a = norm;
    const b = phi * norm;
    return [
        [0,  a,  b], [0,  a, -b], [0, -a,  b], [0, -a, -b],
        [ a,  b, 0], [ a, -b, 0], [-a,  b, 0], [-a, -b, 0],
        [ b, 0,  a], [-b, 0,  a], [ b, 0, -a], [-b, 0, -a],
    ];
}

// 30 edges of the icosahedron: every pair of vertices whose distance is the edge length
function icosahedronEdges(verts) {
    const edges = [];
    // edge length on a unit icosahedron with these coords:
    // distance between e.g. (0,a,b) and (a,b,0) ; we just take the shortest distances
    // sort all pairwise distances, take the smallest 30
    const pairs = [];
    for (let i = 0; i < verts.length; i++) {
        for (let j = i + 1; j < verts.length; j++) {
            const [ax, ay, az] = verts[i];
            const [bx, by, bz] = verts[j];
            const d2 = (ax - bx) ** 2 + (ay - by) ** 2 + (az - bz) ** 2;
            pairs.push({ i, j, d2 });
        }
    }
    pairs.sort((a, b) => a.d2 - b.d2);
    for (let k = 0; k < 30; k++) edges.push([pairs[k].i, pairs[k].j]);
    return edges;
}

function buildSoccerCanvas(size) {
    // equirectangular: width = 2 * height
    const W = size * 2;
    const H = size;
    const canvas = document.createElement('canvas');
    canvas.width = W;
    canvas.height = H;
    const ctx = canvas.getContext('2d');

    // background: warm-white "leather"
    ctx.fillStyle = '#f6efdf';
    ctx.fillRect(0, 0, W, H);

    const verts = icosahedronVertices();
    const edges = icosahedronEdges(verts);

    // black pentagon caps (drawn as circular blobs at vertices — round enough that they
    // read as pentagons on the curved surface)
    const blobAngleRadius = 0.42; // ~24° geodesic radius
    const cosBlob = Math.cos(blobAngleRadius);

    // edge "seam" lines — geodesic arcs between adjacent vertices, drawn thin & dark
    const seamAngleHalfWidth = 0.018; // ~1° half-thickness
    const cosSeam = Math.cos(seamAngleHalfWidth);

    const img = ctx.getImageData(0, 0, W, H);
    const data = img.data;

    for (let y = 0; y < H; y++) {
        const v = y / (H - 1);
        const theta = v * Math.PI; // 0..π (latitude)
        const sinT = Math.sin(theta);
        const cosT = Math.cos(theta);

        for (let x = 0; x < W; x++) {
            const u = x / W;
            const ph = u * 2 * Math.PI;
            const px = sinT * Math.cos(ph);
            const py = cosT;
            const pz = sinT * Math.sin(ph);

            // 1) pentagon blob test
            let maxDot = -1;
            for (let i = 0; i < verts.length; i++) {
                const d = px * verts[i][0] + py * verts[i][1] + pz * verts[i][2];
                if (d > maxDot) maxDot = d;
            }
            let r, g, b;
            if (maxDot > cosBlob) {
                r = 0x14; g = 0x11; b = 0x0d;
            } else {
                // 2) seam line test (great circle arc between vertex i and vertex j,
                //    if and only if point is "between" them on the arc)
                let onSeam = false;
                for (let e = 0; e < edges.length; e++) {
                    const [i, j] = edges[e];
                    const A = verts[i], B = verts[j];
                    // n = unit normal to the great circle (A x B), normalized
                    const nx = A[1] * B[2] - A[2] * B[1];
                    const ny = A[2] * B[0] - A[0] * B[2];
                    const nz = A[0] * B[1] - A[1] * B[0];
                    const nLen = Math.hypot(nx, ny, nz);
                    if (nLen === 0) continue;
                    const distSin = (px * nx + py * ny + pz * nz) / nLen;
                    // |distSin| ≈ |sin(angle)| from great-circle plane
                    if (Math.abs(distSin) > seamAngleHalfWidth) continue;

                    // ensure point lies on the short arc: project onto plane, check
                    // that its dot products with A and B are both above cos(edgeAngle/2 + small)
                    const arcDotA = px * A[0] + py * A[1] + pz * A[2];
                    const arcDotB = px * B[0] + py * B[1] + pz * B[2];
                    // edge angle for icosahedron unit vertices is ~63.43°; half is ~31.7°
                    // so points on the arc satisfy both arcDotA > cos(31.7°) AND arcDotB > cos(31.7°)
                    if (arcDotA > 0.65 && arcDotB > 0.65) { onSeam = true; break; }
                }
                if (onSeam) {
                    r = 0x14; g = 0x11; b = 0x0d;
                } else {
                    r = 0xf6; g = 0xef; b = 0xdf;
                }
            }
            const idx = (y * W + x) * 4;
            data[idx] = r;
            data[idx + 1] = g;
            data[idx + 2] = b;
            data[idx + 3] = 255;
        }
    }

    ctx.putImageData(img, 0, 0);
    return canvas;
}

async function start() {
    const ballCanvases = Array.from(document.querySelectorAll('canvas.ball3d'));
    const goalCanvases = Array.from(document.querySelectorAll('canvas.goal3d'));
    if (ballCanvases.length === 0 && goalCanvases.length === 0) return;

    let THREE;
    try {
        THREE = await import('three');
    } catch (e) {
        ballCanvases.forEach((c) => fallback(c, e));
        goalCanvases.forEach((c) => fallback(c, e));
        return;
    }

    const texture = await getSoccerTexture(THREE);

    ballCanvases.forEach((canvas) => {
        try {
            mountBall(THREE, canvas, texture);
        } catch (e) {
            fallback(canvas, e);
        }
    });

    goalCanvases.forEach((canvas) => {
        try {
            mountGoal(THREE, canvas, texture);
        } catch (e) {
            fallback(canvas, e);
        }
    });
}

function mountBall(THREE, canvas, soccerTexture) {
    const initialSize = canvas.clientWidth || canvas.width || 200;

    const renderer = new THREE.WebGLRenderer({
        canvas,
        antialias: true,
        alpha: true,
        powerPreference: 'low-power',
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setSize(initialSize, initialSize, false);

    const scene = new THREE.Scene();

    const camera = new THREE.PerspectiveCamera(38, 1, 0.1, 100);
    camera.position.set(0, 0, 4.2);

    const isMini = canvas.classList.contains('ball-mini');

    const geometry = new THREE.SphereGeometry(isMini ? 1.45 : 1.3, 64, 48);
    const material = new THREE.MeshStandardMaterial({
        map: soccerTexture,
        roughness: 0.55,
        metalness: 0.0,
    });
    const ball = new THREE.Mesh(geometry, material);
    scene.add(ball);

    let shadow = null;
    if (!isMini) {
        const shadowGeom = new THREE.CircleGeometry(0.95, 32);
        const shadowMat = new THREE.MeshBasicMaterial({
            color: 0x000000,
            transparent: true,
            opacity: 0.18,
        });
        shadow = new THREE.Mesh(shadowGeom, shadowMat);
        shadow.rotation.x = -Math.PI / 2;
        shadow.position.y = -1.55;
        shadow.scale.set(1, 0.55, 1);
        scene.add(shadow);
    }

    scene.add(new THREE.AmbientLight(0xfff0d0, 0.65));
    const key = new THREE.DirectionalLight(0xfff5d8, 0.95);
    key.position.set(3, 5, 4);
    scene.add(key);
    const fill = new THREE.DirectionalLight(0xc8d8ff, 0.35);
    fill.position.set(-4, -1, 2);
    scene.add(fill);

    if ('ResizeObserver' in window) {
        new ResizeObserver(() => {
            const s = canvas.clientWidth;
            if (s > 0) renderer.setSize(s, s, false);
        }).observe(canvas);
    }

    const startT = performance.now();
    const speed = isMini ? 1.6 : 0.7;
    function animate(now) {
        const t = (now - startT) / 1000;
        ball.rotation.y = t * speed;
        ball.rotation.x = Math.sin(t * 0.55) * (isMini ? 0.25 : 0.35);
        if (!isMini) {
            ball.position.y = Math.sin(t * 2.2) * 0.12;
            if (shadow) {
                shadow.scale.set(1 - Math.abs(Math.sin(t * 2.2)) * 0.08, 0.55, 1);
                shadow.material.opacity = 0.22 - Math.abs(Math.sin(t * 2.2)) * 0.08;
            }
        }
        renderer.render(scene, camera);
        requestAnimationFrame(animate);
    }
    requestAnimationFrame(animate);
}

// ---------------------------------------------------------------------------
// mountGoal: a 3D mini-scene with a goal frame, net, and a ball that flies
// into the top corner on a loop. Used as a header decoration above the
// leaderboard.

function buildNetPanel(THREE, w, h, cols, rows, material) {
    const points = [];
    for (let i = 0; i <= cols; i++) {
        const x = -w / 2 + (w * i) / cols;
        points.push(x, -h / 2, 0, x, h / 2, 0);
    }
    for (let j = 0; j <= rows; j++) {
        const y = -h / 2 + (h * j) / rows;
        points.push(-w / 2, y, 0, w / 2, y, 0);
    }
    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
    return new THREE.LineSegments(geom, material);
}

function buildGoal(THREE) {
    const group = new THREE.Group();
    const W = 6, H = 2.4, D = 1.8;
    const POST_R = 0.07;

    const postMat = new THREE.MeshStandardMaterial({
        color: 0xfafafa, roughness: 0.5, metalness: 0.05,
    });
    const postGeom = new THREE.CylinderGeometry(POST_R, POST_R, H, 16);
    const left = new THREE.Mesh(postGeom, postMat);
    left.position.set(-W / 2, H / 2, 0);
    const right = new THREE.Mesh(postGeom, postMat);
    right.position.set(W / 2, H / 2, 0);
    group.add(left, right);

    const barGeom = new THREE.CylinderGeometry(POST_R, POST_R, W, 16);
    const crossbar = new THREE.Mesh(barGeom, postMat);
    crossbar.rotation.z = Math.PI / 2;
    crossbar.position.set(0, H, 0);
    group.add(crossbar);

    // small angled supports going back to the bottom rear corners
    const supportGeom = new THREE.CylinderGeometry(POST_R * 0.6, POST_R * 0.6, Math.hypot(H, D), 12);
    const supportMat = postMat;
    const supportAngle = Math.atan2(D, H);
    const lSup = new THREE.Mesh(supportGeom, supportMat);
    lSup.position.set(-W / 2, H / 2, -D / 2);
    lSup.rotation.x = supportAngle;
    const rSup = new THREE.Mesh(supportGeom, supportMat);
    rSup.position.set(W / 2, H / 2, -D / 2);
    rSup.rotation.x = supportAngle;
    group.add(lSup, rSup);

    // Net panels — back, top, left, right
    const netMat = new THREE.LineBasicMaterial({
        color: 0xe8e8e8,
        transparent: true,
        opacity: 0.55,
    });
    const back = buildNetPanel(THREE, W, H, 14, 8, netMat);
    back.position.set(0, H / 2, -D);
    group.add(back);

    const top = buildNetPanel(THREE, W, D, 14, 4, netMat);
    top.rotation.x = Math.PI / 2;
    top.position.set(0, H, -D / 2);
    group.add(top);

    const leftNet = buildNetPanel(THREE, D, H, 4, 8, netMat);
    leftNet.rotation.y = Math.PI / 2;
    leftNet.position.set(-W / 2, H / 2, -D / 2);
    group.add(leftNet);

    const rightNet = buildNetPanel(THREE, D, H, 4, 8, netMat);
    rightNet.rotation.y = -Math.PI / 2;
    rightNet.position.set(W / 2, H / 2, -D / 2);
    group.add(rightNet);

    return { group, W, H, D };
}

function mountGoal(THREE, canvas, soccerTexture) {
    const W = canvas.clientWidth || canvas.width || 600;
    const H = canvas.clientHeight || canvas.height || 220;

    const renderer = new THREE.WebGLRenderer({
        canvas, antialias: true, alpha: true, powerPreference: 'low-power',
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setSize(W, H, false);

    const scene = new THREE.Scene();

    const camera = new THREE.PerspectiveCamera(38, W / H, 0.1, 100);
    camera.position.set(1.6, 2.1, 7.0);
    camera.lookAt(-0.6, 1.2, -0.5);

    // pitch
    const pitchMat = new THREE.MeshStandardMaterial({
        color: 0x2e7d4a, roughness: 0.95, metalness: 0,
    });
    const pitch = new THREE.Mesh(new THREE.PlaneGeometry(40, 30), pitchMat);
    pitch.rotation.x = -Math.PI / 2;
    pitch.position.set(0, 0, -3);
    scene.add(pitch);

    // pitch stripe overlay for that mown look
    const stripeMat = new THREE.MeshBasicMaterial({
        color: 0xffffff, transparent: true, opacity: 0.04,
    });
    for (let i = -10; i <= 10; i += 2) {
        const stripe = new THREE.Mesh(new THREE.PlaneGeometry(40, 1), stripeMat);
        stripe.rotation.x = -Math.PI / 2;
        stripe.position.set(0, 0.001, i);
        scene.add(stripe);
    }

    const goal = buildGoal(THREE);
    scene.add(goal.group);

    // Ball
    const ballRadius = 0.32;
    const ball = new THREE.Mesh(
        new THREE.SphereGeometry(ballRadius, 48, 36),
        new THREE.MeshStandardMaterial({ map: soccerTexture, roughness: 0.55, metalness: 0.0 })
    );
    scene.add(ball);

    // Ball shadow
    const ballShadow = new THREE.Mesh(
        new THREE.CircleGeometry(ballRadius * 1.3, 24),
        new THREE.MeshBasicMaterial({ color: 0x000000, transparent: true, opacity: 0.32 })
    );
    ballShadow.rotation.x = -Math.PI / 2;
    scene.add(ballShadow);

    // Goal-flash light, off by default, briefly turns on at impact
    const flash = new THREE.PointLight(0xfff0d0, 0, 12);
    flash.position.set(0, 1.5, 0);
    scene.add(flash);

    // Lights
    scene.add(new THREE.AmbientLight(0xffeed0, 0.5));
    const sun = new THREE.DirectionalLight(0xfff6e0, 1.0);
    sun.position.set(4, 7, 5);
    scene.add(sun);
    const rim = new THREE.DirectionalLight(0xc8d8ff, 0.3);
    rim.position.set(-5, 2, 2);
    scene.add(rim);

    if ('ResizeObserver' in window) {
        new ResizeObserver(() => {
            const w = canvas.clientWidth;
            const h = canvas.clientHeight;
            if (w > 0 && h > 0) {
                renderer.setSize(w, h, false);
                camera.aspect = w / h;
                camera.updateProjectionMatrix();
            }
        }).observe(canvas);
    }

    // Trajectory waypoints (in seconds within a cycle)
    const CYCLE = 3.8;    // seconds per shot
    const TRAVEL = 1.8;   // ball flight duration
    const SETTLE = 0.6;   // ball nested in net
    const RESET = 0.3;    // brief pause off-screen
    // remainder is "ball at rest" before the next shot

    const start = (0.3 + 0.0) * 0; // start position offset on ground
    const startPos = new THREE.Vector3(1.0, ballRadius, 5.5);
    const endPos = new THREE.Vector3(-2.4, 1.7, -1.3); // top-left corner inside net

    // If the URL has ?shot=1, freeze the animation at a known "ball mid-flight"
    // frame so screenshots show the scene clearly instead of the start position.
    const isShotMode = typeof window !== 'undefined' &&
        window.location && window.location.search.indexOf('shot=1') >= 0;
    const SHOT_FROZEN_T = 1.55; // ~0.4s settle in + ~1.15s into 1.8s travel = late mid-flight

    const startT = performance.now();
    function animate(now) {
        const tt = isShotMode
            ? SHOT_FROZEN_T
            : ((now - startT) / 1000) % CYCLE;
        let bx, by, bz, hidden = false, flashIntensity = 0;

        if (tt < 0.4) {
            // wait at start (player approach)
            bx = startPos.x;
            by = startPos.y;
            bz = startPos.z;
        } else if (tt < 0.4 + TRAVEL) {
            // shot
            const u = (tt - 0.4) / TRAVEL;
            const eu = u; // linear interpolation
            bx = startPos.x + (endPos.x - startPos.x) * eu;
            bz = startPos.z + (endPos.z - startPos.z) * eu;
            // parabolic vertical: start at startPos.y, peak ~ 2.0, end at endPos.y
            const peak = 2.0;
            by = (1 - u) * (1 - u) * startPos.y + 2 * (1 - u) * u * peak + u * u * endPos.y;
            // spin
            const speed = 18 * (1 - u * 0.3);
            ball.rotation.x += 0.016 * speed;
            ball.rotation.y += 0.016 * speed * 0.6;
        } else if (tt < 0.4 + TRAVEL + SETTLE) {
            // ball in net, small bobbing
            const u = (tt - 0.4 - TRAVEL) / SETTLE;
            bx = endPos.x;
            by = endPos.y + Math.sin(u * Math.PI * 3) * 0.08 * (1 - u);
            bz = endPos.z + Math.sin(u * Math.PI * 2) * 0.05 * (1 - u);
            flashIntensity = Math.max(0, 2.0 * (1 - u * 2));
        } else if (tt < 0.4 + TRAVEL + SETTLE + RESET) {
            // hide ball briefly (reset)
            hidden = true;
            bx = startPos.x;
            by = startPos.y;
            bz = startPos.z;
        } else {
            // hold at start until cycle ends
            bx = startPos.x;
            by = startPos.y;
            bz = startPos.z;
        }

        ball.visible = !hidden;
        ball.position.set(bx, by, bz);

        // shadow follows on ground
        ballShadow.position.set(bx, 0.005, bz);
        const shadowScale = Math.max(0.4, 1.4 - by * 0.35);
        ballShadow.scale.set(shadowScale, shadowScale, 1);
        ballShadow.material.opacity = Math.max(0.05, 0.32 - by * 0.06);
        ballShadow.visible = !hidden;

        flash.intensity = flashIntensity;

        renderer.render(scene, camera);
        requestAnimationFrame(animate);
    }
    requestAnimationFrame(animate);
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start);
} else {
    start();
}
