/**
 * Here we can see how typescript handles various aspects.
 * The goal is to get as close as possible to typescripts behavior.
 */
/**
 * keyof
 */
// This is a a special case in typescript
type KeyofAny = keyof any; // string | number | symbol
type KeyofFunction = keyof (() => void); // never

/**
 * Keyof union
 */
type KeyofUnion1 = keyof ({ a: number } | { a: string }); // 'a'

type KeyofUnion2 = keyof ({ a: number } | { b: string }); // never
// Same as:
type KeyofUnion2Distributed = keyof { a: number } & keyof { b: string }; // never

// distributing the keyof like this works most of the time but not always.
// Counter example:
type KeyofUnion3 = keyof ({ a: number } | any); // string | number | symbol
type KeyofUnion3Distributed = keyof { a: number } & keyof any; // "a"

// here it works too
type KeyofUnion4 = keyof ({ a: number } | never); // "a"
type KeyofUnion4Distributed = keyof { a: number } & keyof never; // "a"

/**
 * Keyof intersections
 */
type KeyofIntersection = keyof ({ a: number } & { b: string }); // 'a' | 'b'
type KeyofIntersectionDistributed = keyof { a: number } | keyof { b: string }; // 'a' | 'b'
// Same as above. Holds for normal cases but falls apart for any.
type KeyofIntersection2 = keyof ({ a: number } & any); // string | number | symbol
type KeyofIntersection2Distributed = keyof { a: number } & keyof any; //  "a"

/**
 * Index
 */
type UnionIndexed = ({ a: number } | { a: string })["a"]; // string | number
type UnionIndexedDistributed = { a: number }["a"] | { a: string }["a"]; // string | number

type IntersectionIndexed = ({ a: { b: string } } & { a: { a: number } })["a"]; // {a: string, b: number}
type IntersectionIndexedDistributed = { a: { b: string } }["a"] &
  { a: { a: number } }["a"]; // string | number

/**
 * Distributing conditionals
 */
type Flatten<Type> = Type extends Array<infer Item> ? Item : Type;

type A = Flatten<number[] | string[]>; // number

/**
 *  {
 *      a: number;
 *      c: string;
 *  } | {
 *      a: number;
 *      d: boolean;
 *  } | {
 *      b: number;
 *      c: string;
 *  } | {
 *      b: number;
 *      d: boolean;
 *  }
 */
type IntersectionOfUnions = Prettify<
  ({ a: number } | { b: number }) & ({ c: string } | { d: boolean })
>;

type Prettify<T> = {
  [K in keyof T]: T[K];
} & {};

type ARecord = { a: number }
// any
type TypeIndexedWithAnInvalidType = ARecord[number];
