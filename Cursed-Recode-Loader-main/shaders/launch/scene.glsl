// The menu's world for one frame: sky, clouds, the burning logo, and (WORLD 1) the valley, the river, far ridges.
// Rgb is the light, alpha the distance (the sparks hide behind it). Ported from the preview's FS_SCENE.
layout(local_size_x = 8, local_size_y = 8) in;
layout(set = 0, binding = 1, rgba16f) uniform writeonly image2D Scene;
layout(set = 0, binding = 2) uniform sampler2D uHM;
layout(set = 0, binding = 3) uniform sampler2D uLogoBase;
// The logo: its shape and where it burns hottest come from her logo (uLogoBase); fire, threads, smoke and growth are ours.
const vec2 LB = vec2(.509, .866);
float logoFire(vec2 uv, float t, float grow, out float smokeOut){
  smokeOut = 0.;
  if (uv.x < -.25 || uv.x > 1.25 || uv.y < -.35 || uv.y > 1.1) return 0.;
  vec2 q = uv - LB;
  float r = length(q);
  float th = atan(q.x, -q.y);
  // the fire flows out from the tip: the edge trembles, tongues stretch
  vec2 w = vec2(gn2(vec2(th * 7., r * 9. - t * 1.7)), gn2(vec2(th * 7. + 9.1, r * 9. - t * 1.7)));
  w += .5 * vec2(gn2(vec2(th * 17., r * 22. - t * 2.6)), gn2(vec2(th * 17. + 3.3, r * 22. - t * 2.6)));
  float B = texture(uLogoBase, uv + w * .016 * smoothstep(.02, .25, r)).r;
  vec2 fp = vec2(th * 15., r * 3.2 - t * .8);
  fp.x += .9 * gn2(vec2(th * 4., r * 3. - t * .5));
  float fil = 0., amp = 1.;
  for (int k = 0; k < 3; k++) {
    fil += pow(clamp(1. - abs(gn2(fp) * 2.5), 0., 1.), 10.) * amp;
    fp = fp * vec2(1.9, 1.5) + vec2(5.3, 2.9); amp *= .6;
  }
  float body = smoothstep(.03, .95, B);
  float core = smoothstep(.55, 1., B) * (1. - smoothstep(.1, .5, r));
  float e = body * body * 1.55 * (.4 + .95 * fil) + smoothstep(.05, .45, B) * fil * .8 + core * 1.6;
  float band = smoothstep(.0, .08, B) * smoothstep(.45, .1, B);
  e += pow(clamp(1. - abs(gn2(vec2(th * 22., r * 7. - t * 1.2) + w * 3.) * 3.), 0., 1.), 14.) * band * 1.6;
  // the fire climbs from the tip; the front is ragged and hotter
  float front = grow - .11 * fbm2(vec2(th * 9., r * 4. - t * 2.6)) - .04 * gn2(vec2(th * 30., t * 3.));
  float alive = smoothstep(front + .02, front - .14, r);
  e = e * alive + e * smoothstep(0., .12, alive) * (1. - alive) * 1.4;
  // smoke round the wings rises and melts; it grows softly after the fire
  float sm = fbm2(uv * 4. + vec2(0., t * .12) + .6 * vec2(fbm2(uv * 3. - t * .07), fbm2(uv * 3. + 4.)));
  smokeOut = smoothstep(.52, .95, sm) * smoothstep(.62, .2, length((uv - vec2(.5, .52)) * vec2(.85, 1.))) * .38 * smoothstep(.15, .9, grow - r);
  return e;
}
const float WATER = 9.6;

vec3 gLd;   // toward the light from the camera

// v12: the world loads in chunks of 16 x 16 as a wave from the horizon (the far first, by the camera last), each rising
// block by block out of nothing; and unloads the other way, from the camera
float chunkRise(vec2 c){
  if (uRiseDir == 0.) return 1.;
  vec2 ch = floor(c / 16.);
  float u = clamp((ch.y * 16. + 8. + 70.) / 790., 0., 1.);        // 0 by the camera .. 1 at the horizon
  float j = hash12(ch + 3.7);
  if (uRiseDir > 0.) {
    float ts = pow(1. - u, 1.4) * 1.45 + j * .16;
    float k = clamp((uRiseT - ts) / .5, 0., 1.);
    return 1. - pow(1. - k, 3.);
  }
  float ts = pow(u, .85) * 1.5 + j * .18;
  float k = clamp((uRiseT - ts) / .42, 0., 1.);
  return 1. - k * k * (3. - 2. * k);
}
float hAt(vec2 c){
  ivec2 q = ivec2(c.x + 192., c.y - uZO);
  float h = (q.x < 0 || q.y < 0 || q.x >= 384 || q.y >= 768) ? uOutH : floor(texelFetch(uHM, q, 0).r * 255. + .5);
  if (uRiseDir == 0.) return h;
  float r = chunkRise(c);
  return r <= .001 ? -100. : ceil(h * r);
}

// the air: two layers of fog (the haze and low fog over the water), exact, no steps
float fogDepth(vec3 ro, vec3 rd, float t){
  float acc = 0.;
  for (int i = 0; i < 2; i++) {
    float a = i == 0 ? .0045 : .006, H = i == 0 ? 24. : 3.5;
    float k = rd.y / H;
    float base = a * exp(-(ro.y - WATER) / H);
    acc += abs(k * t) < 1e-3 ? base * t : base * (1. - exp(-k * t)) / k;
  }
  return acc;
}
// the logo's light scattered in the air: strongest forward
float phaseL(vec3 rd){ float mu = max(dot(rd, gLd), 0.); return .005 + 1.1 * pow(mu, 30.) + .2 * pow(mu, 10.) + .02 * mu; }
// light scattered between us and the world; with the logo behind the ridges the near air is no longer lit by it
float dusk(){ return smoothstep(.3, -.04, gLd.y); }
// toward the afterglow along the horizon (1 there), whatever the height
float towardGlow(vec3 rd){ vec2 a = normalize(rd.xz + 1e-5), b = normalize(gLd.xz + 1e-5); return max(dot(a, b), 0.); }
float airLight(vec3 rd){
  float d = dusk();
  float twilight = d * uGlow * (1. + uBoost) * (.016 + .11 * pow(towardGlow(rd), 6.)) * exp(-max(rd.y, 0.) * 3.5);
  return uGlow * (1. + uBoost) * phaseL(rd) * 1.1 * (1. - .85 * d) + twilight + .003 * uLit;
}
// the afterglow: with the logo low or set, a thin bright band lies on the horizon on its side
float afterglow(vec3 rd){
  float low = smoothstep(.3, -.04, gLd.y);
  if (low <= 0.) return 0.;
  vec2 a = normalize(rd.xz + 1e-5), b = normalize(gLd.xz + 1e-5);
  float az = acos(clamp(dot(a, b), -1., 1.));
  float above = max(rd.y - .002, 0.);
  float b1 = exp(-above * 9.) * exp(-az * az * 2.2);
  float b2 = exp(-above * 3.) * exp(-az * az * .7) * .2;
  return low * (b1 + b2) * uGlow * (1. + uBoost + uFlashC * .8) * 1.8;
}

// before the flash light from all sides draws into the ember: thin strokes, faster and faster
float gatherStreaks(vec2 uv){
  if (uGather <= 0. || uIgn >= 0.) return 0.;
  vec2 q = uv - LB;
  float acc = 0.;
  for (int i = 0; i < 30; i++) {
    float fi = float(i);
    float h = hash12(vec2(fi, 3.7)), h2 = hash12(vec2(fi, 9.1));
    float ang = fi / 30. * 6.28318 + (h - .5) * .35;
    float st = h2 * .5;
    float p = clamp((uGather - st) / (1. - st), 0., 1.);
    if (p <= 0.) continue;
    float head = (.32 + .4 * h) * (1. - p) * (1. - p);
    float len = .02 + .2 * p * p;
    vec2 dir = vec2(cos(ang), sin(ang));
    float along = dot(q, dir), across = abs(q.x * dir.y - q.y * dir.x);
    float seg = smoothstep(head - .004, head + .006, along) * smoothstep(head + len, head + len * .3, along);
    float w = .0011 + .0012 * (1. - p);
    // thin threads of light, gone into the ember as they reach it
    acc += seg * exp(-across * across / (w * w)) * (.2 + .75 * p) * (1. - smoothstep(.72, 1., p)) * (1. - clamp((along - head) / len, 0., 1.) * .7);
  }
  return acc;
}
float logoAlong(vec3 ro, vec3 rd, out float smoke){
  smoke = 0.;
  float dn = dot(rd, -uLN);
  if (dn <= 1e-4) return 0.;
  float t = dot(uLC - ro, -uLN) / dn;
  if (t <= 0.) return 0.;
  vec3 q = ro + rd * t - uLC;
  vec2 uv = vec2(dot(q, uLR) / uLS + .5, .5 - dot(q, uLU) / uLS);
  float e = logoFire(uv, uTime, uGrow, smoke) * uLogoI;
  float b = length(uv - LB);
  e += (exp(-b * b / .00022) * 2.4 + exp(-b * 15.) * .3) * uEmber * (1. + .55 * uBeat);   // the ember in the tip
  e += (exp(-b * b / .0004) * 6. + exp(-b * 9.) * .7) * uFlash;   // the flash: the gathered light at once
  e += gatherStreaks(uv);
  return e;
}

// clouds high over the logo, lit from below by the fire
vec3 clouds(vec3 ro, vec3 rd, vec3 sky){
  if (rd.y <= .02) return sky;
  float t = (520. - ro.y) / rd.y;
  vec3 p = ro + rd * t;
  vec2 cp = p.xz * .0032 + vec2(uTime * .006, uTime * .002);
  float d = fbm2(cp + .5 * vec2(fbm2(cp * 1.7 + 3.), fbm2(cp * 1.7 - 5.)));
  float cov = smoothstep(.5, .82, d) * exp(-t * .0007 * uCloudFar) * uClouds;
  if (cov <= 0.) return sky;
  vec3 lp = uLC;
  float near = exp(-length(p.xz - lp.xz) * .0016);
  float lit = max(uLogoI, uLit) * (.08 + 1.5 * near) * (.6 + .4 * fbm2(cp * 4.)) + uGlow * phaseL(rd) * .5;
  float dl = length(p.xz - lp.xz);
  // the flash: light runs over the clouds as a wave
  if (uIgn >= 0. && uIgn < 5.) lit += 1.5 * exp(-pow((dl - uIgn * 900.) / (120. + uIgn * 90.), 2.)) * exp(-uIgn * .8) + 1.4 * exp(-uIgn * 3.) * near;
  // sheet lightning: the fire behind the ridges flares in the clouds
  lit += uFlashC * exp(-dl * .0012) * 2.4;
  return mix(sky, vec3(.006 * uLit + lit * .5), cov * .85);
}

// far ridges: three stepped planes, as if of blocks
float ridgeH(int k, float x){
  float bw = k == 0 ? 2. : (k == 1 ? 3. : 4.);
  float xq = floor(x / bw) * bw;
  float f = k == 0 ? .011 : (k == 1 ? .0075 : .0052);
  float s = float(k) * 17.3 + 2.;
  float n = 0., a = .55, fx = xq * f;
  for (int i = 0; i < 4; i++) { float v = 1. - abs(vn2(vec2(fx, s)) * 2. - 1.); n += v * v * a; fx = fx * 2.1 + 3.7; a *= .5; }
  float base = k == 0 ? 14. : (k == 1 ? 22. : 34.);
  float amp = k == 0 ? 40. : (k == 1 ? 74. : 130.);
  return floor((base + amp * n) / bw) * bw;
}
// the nearest ridge along the ray: t and how far under its crest (for the rim of light)
bool ridges(vec3 ro, vec3 rd, out float tr, out float below, out int kk){
  tr = 1e9; below = 0.; kk = 0;
  if (rd.z <= 1e-3) return false;
  for (int k = 0; k < 3; k++) {
    float rk = W1[k];
    if (rk <= .001) continue;                                          // (v12: the ridges rise after the world)
    float zk = k == 0 ? 330. : (k == 1 ? 470. : 660.);
    if (zk - uCam.z > uFarZ) continue;                                 // past the render distance: no ridge
    float t = (zk - ro.z) / rd.z;
    if (t <= 0.) continue;
    vec3 p = ro + rd * t;
    // a saddle where the logo sets: the glow shows between the peaks
    float gx = uCam.x + (uLC.x - uCam.x) / max(uLC.z - uCam.z, 1.) * (zk - uCam.z);
    float h = ridgeH(k, p.x) * mix(.42, 1., smoothstep(20. + 12. * float(k), 120. + 60. * float(k), abs(p.x - gx)));
    h = floor(h * uRidgeK);
    float bw = k == 0 ? 2. : (k == 1 ? 3. : 4.);
    if (rk < .999) h = floor(h * rk / bw) * bw;
    if (h < 1.) continue;
    if (p.y < h) { tr = t; below = h - p.y; kk = k; return true; }
  }
  return false;
}

bool trace(vec3 ro, vec3 rd, float tMax, out float tHit, out vec3 n, out vec2 cell, out float hc){
  n = vec3(0, 1, 0); tHit = 0.; cell = vec2(0.); hc = 0.;
  if (ro.y > uTopH && rd.y >= 0.) return false;
  vec2 ci = floor(ro.xz);
  vec2 st = sign(rd.xz);
  vec2 inv = 1. / max(abs(rd.xz), vec2(1e-5));
  vec2 tN = (st * (ci - ro.xz) + st * .5 + .5) * inv;
  float t = 0.;
  for (int i = 0; i < 340; i++) {
    float h = hAt(ci);
    float tx = min(tN.x, tN.y);
    if (ro.y + rd.y * t < h) { tHit = t; cell = ci; hc = h; return true; }
    if (ro.y + rd.y * tx < h) { tHit = (h - ro.y) / rd.y; n = vec3(0, 1, 0); cell = ci; hc = h; return true; }
    if (tN.x < tN.y) { t = tN.x; tN.x += inv.x; ci.x += st.x; n = vec3(-st.x, 0, 0); }
    else             { t = tN.y; tN.y += inv.y; ci.y += st.y; n = vec3(0, 0, -st.y); }
    if (t > tMax || (rd.y > 0. && ro.y + rd.y * t > uTopH)) break;
  }
  return false;
}
float shadowRay(vec3 ro, vec3 rd){
  if (rd.y <= 0.) return 0.;
  vec2 ci = floor(ro.xz);
  vec2 st = sign(rd.xz);
  vec2 inv = 1. / max(abs(rd.xz), vec2(1e-5));
  vec2 tN = (st * (ci - ro.xz) + st * .5 + .5) * inv;
  float t = 0.;
  for (int i = 0; i < 48; i++) {
    if (ro.y + rd.y * t < hAt(ci) - .002) return 0.;
    if (tN.x < tN.y) { t = tN.x; tN.x += inv.x; ci.x += st.x; } else { t = tN.y; tN.y += inv.y; ci.y += st.y; }
    if (ro.y + rd.y * t > uTopH) return 1.;
  }
  return 1.;
}

vec3 shadeBlock(vec3 p, vec3 n, vec2 cell, float hc, float dist){
  vec2 uv;
  if (n.y > .5) uv = fract(p.xz); else if (abs(n.x) > .5) uv = vec2(fract(p.z), fract(p.y)); else uv = vec2(fract(p.x), fract(p.y));
  float by = n.y > .5 ? hc - 1. : floor(p.y - n.y * .01);
  vec2 px = floor(uv * 16.);
  // block texture and seams fade with distance so they never shimmer
  float far = smoothstep(25., 110., dist);
  float tex = mix(.8 + .4 * hash13(vec3(px, by * 3.1 + dot(cell, vec2(13.1, 7.7)))), 1., far);
  float alb = (n.y > .5 ? .27 : (by >= hc - 1. ? .2 : .16)) * tex;
  float e = min(min(uv.x, 1. - uv.x), min(uv.y, 1. - uv.y));
  alb *= mix(mix(.62, 1., smoothstep(0., .07, e)), .85, far);
  vec3 L = normalize(uLC - p);
  float lam = max(dot(n, L), 0.);
  if (lam > 0. && uSun > 0.) lam *= shadowRay(p + n * .02, L); else lam = 0.;
  // the logo's light direct; the afterglow soft, from its side
  vec3 Lh = normalize(vec3(L.x, max(L.y, .12), L.z));
  float amb = (.022 + .055 * max(n.y, 0.)) * uLit + uGlow * (1. + uBoost) * (.03 + .16 * max(dot(n, Lh), 0.));
  // the highest terraces still catch the last light once the logo has set
  float last = uGlow * smoothstep(22., 38., p.y) * max(dot(n, Lh), 0.) * .5 * (1. - smoothstep(0., .1, L.y));
  // top faces toward the afterglow catch it on their edge
  float rimTop = max(n.y, 0.) * pow(max(dot(normalize(p - uCam), gLd), 0.), 5.) * uGlow * (1. + uBoost) * .5 * smoothstep(.3, -.04, gLd.y);
  // the dusk: the sky's light from above, the afterglow from its side wrapping round, the steps' corners in shade
  float dk = dusk();
  if (dk > 0.) {
    vec3 Lg = normalize(vec3(gLd.x, 0., gLd.z));
    float wrap = clamp(dot(n, Lg) * .5 + .5, 0., 1.);
    float ao = 1.;
    if (n.y > .5) {
      // a taller neighbour shades the top face along its side
      vec2 f = fract(p.xz);
      ao -= .45 * smoothstep(.5, 0., f.x) * step(hc + .5, hAt(cell + vec2(-1, 0)));
      ao -= .45 * smoothstep(.5, 1., f.x) * step(hc + .5, hAt(cell + vec2(1, 0)));
      ao -= .45 * smoothstep(.5, 0., f.y) * step(hc + .5, hAt(cell + vec2(0, -1)));
      ao -= .45 * smoothstep(.5, 1., f.y) * step(hc + .5, hAt(cell + vec2(0, 1)));
    } else {
      // a wall is darker toward its foot
      float base = hAt(cell + n.xz);
      ao = mix(.45, 1., smoothstep(0., 1.4, p.y - base));
    }
    amb += dk * ((.07 + .08 * max(n.y, 0.)) * uLit + uGlow * (1. + uBoost) * .6 * pow(wrap, 2.5)) * max(ao, .2);
  }
  // the cursor's light: a warm spot in the world under the hand
  vec3 Lc = uCur.xyz - p; float dc = length(Lc);
  float cur = uCur.w * max(dot(n, Lc / max(dc, 1e-3)), 0.) * 45. / (30. + dc * dc);
  vec3 col = vec3(alb * (lam * uSun * 1.7 + amb + last + rimTop + cur));
  // the loading's front: a chunk rising glows along its edges, as if the light drew it
  if (uRiseDir != 0.) {
    float r = chunkRise(cell);
    float sheen = r * (1. - r) * 4.;
    col += vec3(sheen * (n.y > .5 ? .03 + .09 * (1. - smoothstep(0., .05, e)) : .02)) * exp(-dist * .005);
  }
  return col;
}

vec3 ridgeColor(vec3 rd, float below, int k){
  float rim = exp(-below * (k == 0 ? .45 : .3)) * (.03 + 1.5 * pow(max(dot(rd, gLd), 0.), 9.)) * uGlow * (1. + uBoost + uRim + uFlashC);
  // the crest's own edge catches the afterglow behind it
  float edge = exp(-below * 1.6) * pow(towardGlow(rd), 3.) * dusk() * uGlow * .5;
  return vec3((.006 + .005 * float(k)) * uLit + rim * .35 + edge);
}

// Once the logo has set its fire burns on behind the ridges: low clouds over the valley, long streaks, lit from
// beneath by it — their thin edges let its light through, their thick hearts stand dark against the afterglow.
vec3 duskClouds(vec3 ro, vec3 rd, vec3 sky){
  float dk = dusk();
  if (dk <= 0. || rd.y <= .012) return sky;
  float t = (190. - ro.y) / rd.y;
  vec3 p = ro + rd * t;
  vec2 cp = p.xz * vec2(.0021, .0043) + vec2(uTime * .0045, uTime * .0012);
  float d = fbm2(cp + .6 * vec2(fbm2(cp * 1.9 + 3.), fbm2(cp * 1.9 - 5.)));
  float cov = smoothstep(.47, .76, d) * exp(-t * .00032) * dk;
  if (cov <= 0.) return sky;
  float g = pow(towardGlow(rd), 4.) * exp(-max(rd.y, 0.) * 4.5);
  float thin = 1. - smoothstep(.55, .88, d);
  float under = uGlow * (1. + uBoost) * (.025 + 1.25 * g) * (.3 + .7 * thin) + uFlashC * .7 * g;
  return mix(sky, vec3(.004 * uLit + under * .55), cov * .92);
}

// ---- the valley's life (her choice, 15.10; the house and the one with the lantern gone at her word): a dead tree on
// the right bank where crows sit, circle, now and then take off and come back, one passing close now and then, ash
// drifting in the air.
float aHash(float n){ return hash12(vec2(n, n * .618 + 17.3)); }
// A stroke from a to b, its half thickness r.x at a, r.y at b: how far p is outside it.
float segD(vec2 p, vec2 a, vec2 b, vec2 r){
  vec2 pa = p - a, ba = b - a;
  float h = clamp(dot(pa, ba) / dot(ba, ba), 0., 1.);
  return length(pa - ba * h) - mix(r.x, r.y, h);
}

// -- the dead tree on the right bank: its branches strokes in a plane that faces the camera, the top swaying a little
const vec2 TREE = vec2(-31., 84.);
const float TK = 1.5;    // its size
float gTreeY;
bool gTreeOn;
const int TN = 19;
const vec4 TS[TN] = vec4[TN](
  vec4(0., -1.5, .4, 6.), vec4(.4, 6., -.3, 10.5), vec4(-.3, 10.5, .5, 14.5), vec4(.5, 14.5, .2, 17.4),
  vec4(.4, 6., 4.2, 9.2), vec4(4.2, 9.2, 6.9, 9.9), vec4(4.2, 9.2, 5.3, 12.7),
  vec4(-.1, 8.4, -4.5, 11.), vec4(-4.5, 11., -7.3, 11.4), vec4(-4.5, 11., -5.7, 14.3),
  vec4(-.3, 10.5, 3., 13.8), vec4(3., 13.8, 4.7, 16.3), vec4(3., 13.8, 5.7, 14.1),
  vec4(.5, 14.5, -2.6, 16.6), vec4(-2.6, 16.6, -3.4, 18.1), vec4(.3, 12.3, -1.9, 13.5),
  vec4(-7.3, 11.4, -8., 12.6), vec4(6.9, 9.9, 7.6, 11.), vec4(.2, 17.4, -.4, 18.6));
const vec2 TR[TN] = vec2[TN](
  vec2(.95, .62), vec2(.62, .44), vec2(.44, .24), vec2(.24, .07),
  vec2(.34, .13), vec2(.13, .05), vec2(.1, .03),
  vec2(.3, .11), vec2(.11, .04), vec2(.09, .03),
  vec2(.22, .08), vec2(.08, .025), vec2(.07, .025),
  vec2(.15, .04), vec2(.045, .015), vec2(.12, .04),
  vec2(.04, .015), vec2(.05, .015), vec2(.07, .02));
// where the crows sit (on the branches; each sitter its own)
const vec2 PERCH[4] = vec2[4](vec2(6.1, 9.75), vec2(-6.4, 11.3), vec2(4.9, 14.05), vec2(-2.95, 16.9));
float sway(float y){ float k = clamp(y / 18., 0., 1.); return k * k * (.16 * sin(uTime * .53) + .05 * sin(uTime * 1.31 + 1.7)); }

// A crow sitting, hunched on its branch (its picture plane, world units, y up; facing `hd` ±1).
float crowSit(vec2 p, float hd){
  p.x *= hd;
  float d = segD(p, vec2(-.05, .12), vec2(.06, .55), vec2(.22, .19));
  d = min(d, length(p - vec2(.16, .72)) - .13);
  d = min(d, segD(p, vec2(.22, .73), vec2(.42, .69), vec2(.05, .01)));
  d = min(d, segD(p, vec2(-.12, .2), vec2(-.4, -.18), vec2(.1, .03)));
  return d;
}
// A crow flying: its wings an "M" beating (`ph` their phase), held a little up while it glides (`gl` 0..1); `w` its
// half wingspan, `hd` which way it goes across the picture.
float crowFly(vec2 p, float w, float hd, float ph, float gl){
  float up = mix(sin(ph), .3, gl);
  vec2 q = p / w;
  q.x *= hd;
  float d = segD(q, vec2(-.14, 0.), vec2(.14, .02), vec2(.075, .05));
  d = min(d, length(q - vec2(.2, .035)) - .05);
  d = min(d, segD(q, vec2(-.14, 0.), vec2(-.32, -.03), vec2(.05, .02)));
  for (int s = 0; s < 2; s++) {
    float fs = s == 0 ? -1. : 1.;
    vec2 el = vec2(fs * .45, .2 * up + .04);
    vec2 tp = vec2(fs * 1., .52 * up - .06);
    d = min(d, segD(q, vec2(fs * .04, .01), el, vec2(.085, .06)));
    d = min(d, segD(q, el, tp, vec2(.06, .014)));
  }
  return d * w;
}

// A sitter's flight now: -1 while it sits; else 0..1 along its loop round the tree.
float sitterF(int j){
  float fj = float(j);
  // the launch while it is a drawing: they sit, all of them
  if (Comp3.w > .5) return -1.;
  // startled when it came alive (Comp3.z: the valley's second then, -1 never): all up from the tree at once, a loop
  // round it, and back onto their branches
  float dur0 = 12. + fj * 1.5;
  float back = Comp3.z + fj * .18 + dur0;
  if (Comp3.z >= 0.) {
    float f = (uTime - Comp3.z - fj * .18) / dur0;
    if (f >= 0. && f < 1.) return f;
  }
  float cyc = 41. + fj * 13.;
  float tt = uTime + fj * 9.;
  float k = floor(tt / cyc), lt = tt - k * cyc;
  if (aHash(k * 1.3 + fj * 7. + 90.) < .3) return -1.;
  float t0 = 4. + aHash(k + fj * 5. + 91.) * (cyc - 26.);
  float D = 15. + aHash(k + fj + 92.) * 6.;
  if (lt < t0 || lt > t0 + D) return -1.;
  // (a flight of its own begun before it was back from the startle's: sat out — no leap off or onto its branch)
  if (Comp3.z >= 0. && uTime - (lt - t0) < back) return -1.;
  return (lt - t0) / D;
}
// Up from its branch, a wide loop round the tree, and back down onto it.
vec3 loopPath(vec3 P, float f, float side, out vec3 vel){
  float a = f * 6.2832, R = 8.;
  vec3 c = P + vec3(side * R, 0., 0.);
  vel = vec3(side * sin(a) * R * 6.2832, cos(f * 3.1416) * 8. * 3.1416 + cos(a) * 1.2 * 6.2832, cos(a) * R * .9 * 6.2832);
  return c + vec3(-side * cos(a) * R, sin(f * 3.1416) * 8. + sin(a) * 1.2, sin(a) * R * .9);
}
// The ones always up: circling lazily high over the tree.
vec3 orbitCrow(int i, out vec3 vel){
  float fi = float(i);
  float R = 10. + aHash(fi + 50.) * 13.;
  float w = (.17 + aHash(fi + 51.) * .12) * (aHash(fi + 54.) < .2 ? -1. : 1.);
  float a = uTime * w + aHash(fi + 52.) * 6.283;
  float hh = 9. + aHash(fi + 53.) * 13. + 2.2 * sin(uTime * .19 + fi * 1.7);
  vec3 c = vec3(TREE.x - 5., gTreeY + 18. + hh, TREE.y + 12.);
  vel = vec3(-sin(a), 0., cos(a) * .8) * R * w;
  return c + vec3(cos(a) * R, 0., sin(a) * R * .8);
}
void crowAt(vec3 ro, vec3 rd, vec3 P, vec3 vel, float w, float ph, float gl, float pxk, inout vec3 col, inout float depth){
  vec3 v = P - ro;
  float al = dot(v, rd);
  if (al <= 0. || al > depth) return;
  float fw = al * pxk;
  if (length(v - rd * al) > w * 1.2 + fw * 2.) return;
  vec3 q = rd * al - v;
  vec2 p = vec2(dot(q, uRight), dot(q, uUp));
  float hd = dot(vel, uRight) >= 0. ? 1. : -1.;
  float cov = clamp(.5 - crowFly(p, w, hd, ph, gl) / fw, 0., 1.);
  if (cov <= 0.) return;
  float T = exp(-fogDepth(ro, rd, al) * uFog);
  col = mix(col, vec3(.0015) * T + vec3(airLight(rd) * (1. - T)), cov);
  if (cov > .5) depth = al;
}

// -- ash: a few grey flakes falling slowly, drifting with the valley's air, turning (they catch the light as they do)
float ash(vec3 rd, float depth, float pxk){
  float fz = dot(rd, uFwd);
  if (fz <= .05) return 0.;
  vec2 sp = vec2(dot(rd, uRight), dot(rd, uUp)) / fz;
  float a = 0.;
  for (int L = 0; L < 3; L++) {
    float fl = float(L);
    float D = L == 0 ? 4.5 : (L == 1 ? 10. : 21.);
    if (depth < D) continue;
    float S = 1.1 + .8 * fl;
    vec2 g = sp * D + vec2(uTime * (.16 + .05 * fl), uTime * (.3 + .07 * fl));
    vec2 c = floor(g / S);
    float h = hash12(c + fl * 31.7);
    if (h > .42) continue;
    vec2 at = (vec2(hash12(c + 3.1), hash12(c + 7.7)) * .7 + .15) * S;
    at.x += .1 * S * sin(uTime * (.6 + h) + h * 40.);
    float d = length(g - c * S - at);
    float fw = D * pxk / fz;
    float r = .022 + .026 * h;
    float rad = max(r, fw * .75);
    float cov = clamp(1. - d / rad, 0., 1.) * min(1., r / rad * 1.6);
    float turn = .3 + .7 * abs(sin(uTime * (1.1 + 2. * h) + h * 50.));
    a += cov * turn;
  }
  return min(a, 1.);
}

void lifeSetup(){
  gTreeOn = TREE.y - uCam.z <= uFarZ && (uRiseDir == 0. || chunkRise(floor(TREE)) > .99);
  gTreeY = hAt(floor(TREE));
}

void life(vec3 ro, vec3 rd, inout vec3 col, inout float depth){
  float pxk = uTanH / (Size.y * .5);
  // the tree, the crows sitting on it
  if (gTreeOn) {
    vec3 P0 = vec3(TREE.x, gTreeY, TREE.y);
    vec3 nT = normalize(vec3(ro.x - P0.x, 0., ro.z - P0.z));
    vec3 rT = vec3(-nT.z, 0., nT.x);
    float dn = dot(rd, nT);
    if (dn < -1e-4) {
      float t = dot(P0 - ro, nT) / dn;
      if (t > 0. && t < depth) {
        vec3 q = ro + rd * t - P0;
        vec2 p = vec2(dot(q, rT), q.y) / TK;
        if (p.x > -9. && p.x < 9. && p.y > -2. && p.y < 20.) {
          float fw = t * pxk / TK;
          vec2 ps = vec2(p.x - sway(p.y), p.y);
          float d = 1e9;
          for (int i = 0; i < TN; i++) d = min(d, segD(ps, TS[i].xy, TS[i].zw, TR[i]));
          for (int j = 0; j < 4; j++) {
            if (sitterF(j) >= 0.) continue;
            vec2 pc = PERCH[j];
            pc.x += sway(pc.y);
            d = min(d, crowSit(p - pc, j % 2 == 0 ? 1. : -1.));
          }
          float cov = clamp(.5 - d / fw, 0., 1.);
          if (cov > 0.) {
            float T = exp(-fogDepth(ro, rd, t) * uFog);
            col = mix(col, vec3(.0022 + .002 * uLit) * T + vec3(airLight(rd) * (1. - T)), cov);
            if (cov > .5) depth = t;
          }
        }
      }
    }
    // the crows up: those circling, the sitters away on their loops
    for (int i = 0; i < 7; i++) {
      vec3 vel;
      vec3 P = orbitCrow(i, vel);
      float fi = float(i);
      float gl = smoothstep(.2, .5, sin(uTime * (.31 + .07 * fi) + fi * 2.1));
      crowAt(ro, rd, P, vel, 1.45, uTime * 6.2832 * (2.9 + .4 * aHash(fi + 60.)) + fi, gl, pxk, col, depth);
    }
    for (int j = 0; j < 4; j++) {
      float f = sitterF(j);
      if (f < 0.) continue;
      vec2 pc = PERCH[j];
      vec3 P = P0 + (rT * pc.x + vec3(0., pc.y + .45, 0.)) * TK;
      vec3 vel;
      vec3 Q = loopPath(P, f, j % 2 == 0 ? 1. : -1., vel);
      float busy = 1. - smoothstep(.1, .2, f) * (1. - smoothstep(.8, .9, f));
      float gl = (1. - busy) * smoothstep(.3, .6, sin(uTime * .9 + float(j)));
      crowAt(ro, rd, Q, vel, 1.2, uTime * 6.2832 * mix(3.1, 5.2, busy) + float(j), gl, pxk, col, depth);
    }
  }
  // now and then one passes close, high over the river, right to left
  {
    float cyc = 71.;
    float k = floor(uTime / cyc), lt = uTime - k * cyc;
    if (aHash(k + 120.) > .35 && lt < 7.5) {
      vec3 P = vec3(-22. + lt * 6.1, 27.6 + .8 * sin(lt * 1.1), 31.);
      float gl = smoothstep(.2, .6, sin(lt * 1.3 + 1.));
      crowAt(ro, rd, P, vec3(6.1, 0., 0.), 1.25, uTime * 6.2832 * 2.4, gl, pxk, col, depth);
    }
  }
  // the ash, nearest last
  float a = ash(rd, depth, pxk);
  if (a > 0.) col = mix(col, vec3(.3 + .45 * airLight(rd)), a * .75);
}

vec3 skyCol(vec3 ro, vec3 rd, bool withLogo){
  float F = fogDepth(ro, rd, 3000.) * uFog;
  float T = exp(-F);
  vec3 c = vec3(airLight(rd) * (1. - T) + afterglow(rd));
  c = clouds(ro, rd, c);
  c = duskClouds(ro, rd, c);
  if (withLogo) {
    float sm; float e = logoAlong(ro, rd, sm);
    c += vec3(e) * mix(1., T, .8) + vec3(sm * uLogoI * .5);
  }
  return c;
}

void main(){
  ivec2 q = ivec2(gl_GlobalInvocationID.xy);
  if (q.x >= int(Size.x) || q.y >= int(Size.y)) return;
  vec2 frag = vec2(float(q.x) + .5, Size.y - float(q.y) - .5);
  vec2 uv = frag / Size.xy * 2. - 1.;
  vec3 ro = uCam;
  vec3 rd = normalize(uFwd + uv.x * uTanH * uAspect * uRight + uv.y * uTanH * uUp);
  gLd = normalize(uLC - ro);
#if WORLD == 1
  lifeSetup();
#endif
  vec3 col;
  float depth = 1e4;
  // (v12: nothing of the world until the wings have lain down as its horizon)
  if (W0.w < .5) { imageStore(Scene, q, vec4(0., 0., 0., 1e4)); return; }
#if WORLD == 0
  col = skyCol(ro, rd, true);
#else
  float tHit; vec3 n; vec2 cell; float hc;
  bool hit = trace(ro, rd, min(300., uFarZ), tHit, n, cell, hc);
  float tS = hit ? tHit : 1e9;
  float tR, below; int kk;
  bool rh = ridges(ro, rd, tR, below, kk);
  if (rh && tR < tS) { tS = tR; }
  float tW = rd.y < 0. ? (WATER - ro.y) / rd.y : 1e9;
  // the water only over the chunks that are there
  if (tW < tS && uRiseDir != 0.) { vec3 pw0 = ro + rd * tW; if (chunkRise(floor(pw0.xz)) < .55) tW = 1e9; }
  if (tW < tS) {
    // the river and the lake reflect the afterglow, ridges, logo; slopes shade the banks
    vec3 pw = ro + rd * tW;
    vec2 wv = vec2(fbm2(pw.xz * .25 + vec2(uTime * .16, 0.)), fbm2(pw.xz * .25 + vec2(3.7, uTime * .14))) - .5;
    vec2 wv2 = vec2(vn2(pw.xz * 1.3 + uTime * .5), vn2(pw.xz * 1.3 - uTime * .4 + 7.)) - .5;
    vec3 wn = normalize(vec3(wv.x * .12 + wv2.x * .05, 1., wv.y * .12 + wv2.y * .05));
    vec3 rr = reflect(rd, wn);
    float fr = .03 + .97 * pow(1. - max(dot(-rd, wn), 0.), 5.);
    vec3 refl;
    float tr2, bl2; int k2;
    bool rr2 = uRefl > .5 && ridges(pw, rr, tr2, bl2, k2);
    if (rr2) {
      float T2 = exp(-fogDepth(pw, rr, tr2) * uFog);
      refl = ridgeColor(rr, bl2, k2) * T2 + vec3(airLight(rr) * (1. - T2));
    } else refl = skyCol(pw, rr, true);
    // near slopes hide the sky in the reflection
    float occ = 1.;
    for (int i = 1; i <= 12; i++) {
      vec3 s = pw + rr * (float(i) * float(i) * .9);
      if (s.y < hAt(floor(s.xz))) { occ = 0.; break; }
      if (s.y > uTopH) break;
    }
    refl *= mix(.05, 1., occ) * mix(.35, 1., uRefl);
    col = vec3(.004 * uLit) + refl * (.4 + .6 * fr);
    // the afterglow's path: the ripples catch it in a band toward the saddle
    float path = pow(towardGlow(rr), 40.) * smoothstep(-.02, .08, rr.y) * exp(-max(rr.y, 0.) * 7.);
    col += vec3(path * dusk() * uGlow * (1. + uBoost) * 1.4 * occ);
    if (uCur.w > 0.) {
      vec3 Lc = uCur.xyz - pw; float dc = length(Lc);
      col += vec3(uCur.w * (.6 * 16. / (16. + dc * dc) + 2.4 * pow(max(dot(rr, Lc / dc), 0.), 70.) * 60. / (60. + dc * dc)));
    }
    depth = tW;
  } else if (hit && tHit <= tS) {
    vec3 p = ro + rd * tHit;
    col = shadeBlock(p, n, cell, hc, tHit);
    depth = tHit;
  } else if (rh) {
    col = ridgeColor(rd, below, kk);
    depth = tR;
  } else if (uRiseDir != 0. && rd.y < -.01) {
    // (v12: below, where the chunks are not yet there or gone: nothing — no haze, no mist over it)
    imageStore(Scene, q, vec4(0., 0., 0., 1e4));
    return;
  } else {
    col = skyCol(ro, rd, true);
  }
  if (depth < 1e4) {
    float T = exp(-fogDepth(ro, rd, depth) * uFog * (.75 + .5 * fbm2((ro + rd * depth).xz * .012 + uTime * .01)));
    col = col * T + vec3(airLight(rd) * (1. - T));
    // mist over the river in the dusk, thickest on the water, drifting downstream
    vec3 P = ro + rd * depth;
    float low = exp(-max(P.y - WATER, 0.) / 2.2);
    float wisp = smoothstep(.4, .82, fbm2(P.xz * vec2(.042, .019) + vec2(uTime * .02, uTime * .005)));
    float mist = dusk() * low * wisp * (1. - exp(-depth * .03));
    // (v12: no mist over chunks that are not all there — sinking under the water they would catch all of it)
    if (uRiseDir != 0.) mist *= smoothstep(.6, 1., chunkRise(floor(P.xz)));
    col = mix(col, vec3(airLight(rd) * 3. + .012), clamp(mist * .75, 0., 1.));
  }
  // the cursor lights the fog
  if (uCur.w > 0.) {
    float tc = clamp(dot(uCur.xyz - ro, rd), 0., depth);
    float dd = length(ro + rd * tc - uCur.xyz);
    col += vec3(uCur.w * (.26 * exp(-dd * .3) + .06 * exp(-dd * .055)));
  }
  // the valley's life over it: the tree and its crows, the crows in the air, the ash
  life(ro, rd, col, depth);
#endif
  imageStore(Scene, q, vec4(col, depth));
}