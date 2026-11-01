#version 450
// A finish worked in a weapon's texture space (native/src/cosmetics/render.rs runs CS2's own compositor over it): one
// triangle over the target, each point its texture coordinates and the pattern's, the wear's, the grunge's, the
// overlay's (CS2's transforms, native/src/cosmetics/finish.rs), handed on as CS2's compositor's vertex shader does.

layout(push_constant) uniform Xforms {
    vec4 x[8];
} t;

layout(location = 0) out vec4 vColor;
layout(location = 1) out vec4 vUv;
layout(location = 2) out vec4 vWearGrunge;
layout(location = 3) out vec2 vOverlay;

vec2 at(vec2 uv, int k) {
    return vec2(dot(uv, t.x[k].xy) + t.x[k].w, dot(uv, t.x[k + 1].xy) + t.x[k + 1].w);
}

void main() {
    vec2 p = vec2(float((gl_VertexIndex << 1) & 2), float(gl_VertexIndex & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    vec2 uv = p;
    vColor = vec4(1.0);
    vUv = vec4(uv, at(uv, 0));
    vWearGrunge = vec4(at(uv, 2), at(uv, 4));
    vOverlay = at(uv, 6);
}
