#version 450
// A cosmetic's model in the hands (native/src/cosmetics/render.rs): its vertices moved by their skin (four joints each),
// then placed in the view; the fragment shader lights it.

layout(location = 0) in vec3 inPos;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec4 inTangent;
layout(location = 3) in vec2 inUv;
layout(location = 4) in uvec4 inJoints;
layout(location = 5) in vec4 inWeights;

layout(set = 0, binding = 0) uniform View {
    mat4 proj;
    vec4 sun;       // xyz toward the sun in the view, w how much of it reaches here
    vec4 sunColor;  // rgb, w the sky's light here (0..1)
    vec4 sky;       // the sky's colour overhead (rgb), w the block light here (0..1)
    vec4 horizon;   // the colour at the horizon (rgb), w exposure
    vec4 params;    // x time (s), y the material's look (0 as made), z, w spare
} view;

layout(std430, set = 0, binding = 1) readonly buffer Pose {
    mat4 model;     // the model's place in the view
    mat4 joints[];  // each joint's skin matrix
} pose;

layout(location = 0) out vec3 vPos;
layout(location = 1) out vec3 vNormal;
layout(location = 2) out vec4 vTangent;
layout(location = 3) out vec2 vUv;
// the vertex in the model as bound (our materials' patterns stay on the item as it moves)
layout(location = 4) out vec3 vModel;
// the same depth from either of its pipelines (a reflection's depth laid first, then its colour where it is nearest)
invariant gl_Position;

void main() {
    mat4 skin = inWeights.x * pose.joints[inJoints.x] + inWeights.y * pose.joints[inJoints.y]
              + inWeights.z * pose.joints[inJoints.z] + inWeights.w * pose.joints[inJoints.w];
    mat4 m = pose.model * skin;
    vec4 p = m * vec4(inPos, 1.0);
    vPos = p.xyz;
    mat3 n = mat3(m);
    vNormal = n * inNormal;
    // a place mirrored (the left hand, a reflection) turns the tangents' frame over: its bitangent turned back
    vTangent = vec4(n * inTangent.xyz, determinant(n) < 0.0 ? -inTangent.w : inTangent.w);
    vUv = inUv;
    vModel = inPos;
    gl_Position = view.proj * p;
}
