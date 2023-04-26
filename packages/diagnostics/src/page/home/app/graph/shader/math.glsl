#define PI 3.1415926535897932384626433832795
#define PI2 6.28318530718
#define PI_HALF 1.5707963267949
#define RECIPROCAL_PI 0.31830988618
#define RECIPROCAL_PI2 0.15915494
#define LOG2 1.442695
#define EPSILON 1e-6

#define saturate(a) clamp( a, 0.0, 1.0 )
#define whiteCompliment(a) ( 1.0 - saturate( a ) )

float pow2( const in float x ) { return x*x; }
float pow3( const in float x ) { return x*x*x; }
float pow4( const in float x ) { float x2 = x*x; return x2*x2; }
float average( const in vec3 color ) { return dot( color, vec3( 0.3333 ) ); }

// expects values in the range of [0,1]x[0,1], returns values in the [0,1] range.
// do not collapse into a single function per: http://byteblacksmith.com/improvements-to-the-canonical-one-liner-glsl-rand-for-opengl-es-2-0/
highp float rand( const in vec2 uv ) {
	const highp float a = 12.9898, b = 78.233, c = 43758.5453;
	highp float dt = dot( uv.xy, vec2( a,b ) ), sn = mod( dt, PI );
	return fract(sin(sn) * c);
}

float clamped_dot(vec3 x, vec3 y)
{
    return clamp(dot(x, y), 0.0, 1.0);
}

// logical operator replacements
// vec4
vec4 when_eq_v4(vec4 x, vec4 y) {
  return 1.0 - abs(sign(x - y));
}

vec4 when_neq_v4(vec4 x, vec4 y) {
  return abs(sign(x - y));
}

vec4 when_gt_v4(vec4 x, vec4 y) {
  return max(sign(x - y), 0.0);
}

vec4 when_lt_v4(vec4 x, vec4 y) {
  return max(sign(y - x), 0.0);
}

vec4 when_ge_v4(vec4 x, vec4 y) {
  return 1.0 - when_lt_v4(x, y);
}

vec4 when_le_v4(vec4 x, vec4 y) {
  return 1.0 - when_gt_v4(x, y);
}

vec4 and_v4(vec4 a, vec4 b) {
  return a * b;
}

vec4 or_v4(vec4 a, vec4 b) {
  return min(a + b, 1.0);
}

//vec4 xor_v4(vec4 a, vec4 b) {
  //return (a + b) % 2.0;
//}

vec4 not_v4(vec4 a) {
  return 1.0 - a;
}

// vec3
vec3 when_eq_v3(vec3 x, vec3 y) {
  return 1.0 - abs(sign(x - y));
}

vec3 when_neq_v3(vec3 x, vec3 y) {
  return abs(sign(x - y));
}

vec3 when_gt_v3(vec3 x, vec3 y) {
  return max(sign(x - y), 0.0);
}

vec3 when_lt_v3(vec3 x, vec3 y) {
  return max(sign(y - x), 0.0);
}

vec3 when_ge_v3(vec3 x, vec3 y) {
  return 1.0 - when_lt_v3(x, y);
}

vec3 when_le_v3(vec3 x, vec3 y) {
  return 1.0 - when_gt_v3(x, y);
}

vec3 and_v3(vec3 a, vec3 b) {
  return a * b;
}

vec3 or_v3(vec3 a, vec3 b) {
  return min(a + b, 1.0);
}

//vec3 xor_v3(vec3 a, vec3 b) {
  //return (a + b) % 2.0;
//}

vec3 not_v3(vec3 a) {
  return 1.0 - a;
}

// float

float when_eq_flt(float x, float y) {
  return 1.0 - abs(sign(x - y));
}

float when_neq_flt(float x, float y) {
  return abs(sign(x - y));
}

float when_gt_flt(float x, float y) {
  return max(sign(x - y), 0.0);
}

float when_lt_flt(float x, float y) {
  return max(sign(y - x), 0.0);
}

float when_ge_flt(float x, float y) {
  return 1.0 - when_lt_flt(x, y);
}

float when_le_flt(float x, float y) {
  return 1.0 - when_gt_flt(x, y);
}

float and_flt(float a, float b) {
  return a * b;
}

float or_flt(float a, float b) {
  return min(a + b, 1.0);
}

//float xor_flt(float a, float b) {
  //return (a + b) % 2.0;
//}

float not_flt(float a) {
  return 1.0 - a;
}