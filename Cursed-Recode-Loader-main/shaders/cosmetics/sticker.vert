#version 450
// A sticker over a model's body (native/src/cosmetics/render.rs, CS2's stickers): the body's vertices placed as
// viewmodel.vert places them, and each one's place on the sticker — projected along the sticker's normal from its middle
// in the model as bound, so it follows the body's joints as the body does.

layout(location = 0) in vec3 inPos;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec4 inTangent;
layout(location = 3) in vec2 inUv;
layout(location = 4) in uvec4 inJoints;
layout(location = 5) in vec4 inWeights;

layout(set = 0, binding = 0) uniform View {
    mat4 proj;
    vec4 sun;
    vec4 sunColor;
    vec4 sky;
    vec4 horizon;
    vec4 params;
} view;

layout(std430, set = 0, binding = 1) readonly buffer Pose {
    mat4 model;
    mat4 joints[];
} pose;

// the sticker in the model as bound: its middle (w how far along its normal it reaches, either way), its across and its
// up each over the sticker's own width and height (a point's offset dotted with them, + 0.5: its u, and 0.5 less its v
// — the picture runs down), its normal out of the body; the fragment shader's own after them
layout(push_constant) uniform Sticker {
    vec4 origin;
    vec4 across;
    vec4 up;
    vec4 normal;
} st;

layout(location = 0) out vec3 vPos;
layout(location = 1) out vec3 vNormal;
layout(location = 2) out vec4 vTangent;
layout(location = 3) out vec2 vUv;
// the sticker's u, v and how far out of its plane; how much the body faces its way
layout(location = 4) out vec4 vSticker;
// the sticker's across and up in the view (its normal map's frame)
layout(location = 5) out vec3 vAcross;
layout(location = 6) out vec3 vUp;

void main() {
    mat4 skin = inWeights.x * pose.joints[inJoints.x] + inWeights.y * pose.joints[inJoints.y]
              + inWeights.z * pose.joints[inJoints.z] + inWeights.w * pose.joints[inJoints.w];
    mat4 m = pose.model * skin;
    vec4 p = m * vec4(inPos, 1.0);
    vPos = p.xyz;
    mat3 n = mat3(m);
    vNormal = n * inNormal;
    vTangent = vec4(n * inTangent.xyz, inTangent.w);
    vUv = inUv;
    vec3 d = inPos - st.origin.xyz;
    vSticker = vec4(dot(d, st.across.xyz) + 0.5, 0.5 - dot(d, st.up.xyz), dot(d, st.normal.xyz) / max(st.origin.w, 1e-5), dot(normalize(inNormal), st.normal.xyz));
    vAcross = n * st.across.xyz;
    vUp = n * st.up.xyz;
    gl_Position = view.proj * p;
}
