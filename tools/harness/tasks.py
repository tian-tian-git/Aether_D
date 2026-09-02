# -*- coding: utf-8 -*-
"""M4 基准任务集:20 个确定性小函数任务。

每个任务:
- prompt: 自然语言规格(Aether 版要求契约)
- cases: (args, expected) 测试用例,args/expected 用 Python 值表示,
  Aether 侧按类型转写为字面量(int/bool/str → 字面量;list → (vec ...))
- aether_ref: 参考实现(用于自检,不参与生成)
"""

TASKS = [
    {
        "id": "fib",
        "prompt": "写一个函数 fib(n),返回第 n 个斐波那契数(F(0)=0, F(1)=1)。必须带 :pre (>= n 0) 契约。",
        "cases": [((0,), 0), ((1,), 1), ((10,), 55), ((20,), 6765)],
        "aether_ref": """
(fn fib (n Int) -> Int
  :pre (>= n 0)
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
""",
    },
    {
        "id": "fact",
        "prompt": "写一个函数 fact(n),返回 n 的阶乘。必须带 :pre (>= n 0) 契约。",
        "cases": [((0,), 1), ((5,), 120), ((10,), 3628800)],
        "aether_ref": """
(fn fact (n Int) -> Int
  :pre (>= n 0)
  (if (<= n 1) 1 (* n (fact (- n 1)))))
""",
    },
    {
        "id": "gcd",
        "prompt": "写一个函数 gcd(a, b),返回两非负整数的最大公约数(gcd(a,0)=a)。必须带 :pre (and (> a 0) (>= b 0)) 契约。",
        "cases": [((12, 18), 6), ((17, 5), 1), ((100, 10), 10), ((1, 1), 1)],
        "aether_ref": """
(fn gcd (a Int) (b Int) -> Int
  :pre (and (> a 0) (>= b 0))
  (if (== b 0) a (gcd b (% a b))))
""",
    },
    {
        "id": "sum",
        "prompt": "写一个函数 sum(xs),返回整数数组所有元素之和。",
        "cases": [(([],), 0), (([1, 2, 3],), 6), (([-5, 5],), 0)],
        "aether_ref": """
(fn sum (xs (Vec Int)) -> Int
  (fold (fn (acc Int) (x Int) -> Int (+ acc x)) 0 xs))
""",
    },
    {
        "id": "max-elem",
        "prompt": "写一个函数 max-elem(xs),返回非空整数数组的最大值。必须带 :pre (not (empty? xs)) 契约。",
        "cases": [(([7],), 7), (([3, 9, 2],), 9), (([-1, -5],), -1)],
        "aether_ref": """
(fn max-elem (xs (Vec Int)) -> Int
  :pre (not (empty? xs))
  (fold (fn (acc Int) (x Int) -> Int (max acc x)) (head xs) (tail xs)))
""",
    },
    {
        "id": "filter-even",
        "prompt": "写一个函数 filter-even(xs),返回整数数组中所有偶数的列表,保持原顺序。",
        "cases": [(([1, 2, 3, 4],), [2, 4]), (([],), []), (([1, 3],), [])],
        "aether_ref": """
(fn filter-even (xs (Vec Int)) -> (Vec Int)
  (filter (fn (x Int) -> Bool (== (% x 2) 0)) xs))
""",
    },
    {
        "id": "map-square",
        "prompt": "写一个函数 map-square(xs),返回整数数组每个元素的平方列表。",
        "cases": [(([1, 2, 3],), [1, 4, 9]), (([],), []), (([-2],), [4])],
        "aether_ref": """
(fn map-square (xs (Vec Int)) -> (Vec Int)
  (map (fn (x Int) -> Int (* x x)) xs))
""",
    },
    {
        "id": "reverse",
        "prompt": "写一个函数 reverse(xs),返回整数数组的反序列表。",
        "cases": [(([1, 2, 3],), [3, 2, 1]), (([],), []), (([7],), [7])],
        "aether_ref": """
(fn reverse (xs (Vec Int)) -> (Vec Int)
  (fold (fn (acc (Vec Int)) (x Int) -> (Vec Int) (concat (vec x) acc)) (vec) xs))
""",
    },
    {
        "id": "is-prime",
        "prompt": "写一个函数 is-prime(n),判断 n 是否为质数(n>1)。必须带 :pre (> n 1) 契约。",
        "cases": [((2,), True), ((17,), True), ((15,), False)],
        "aether_ref": """
(fn is-prime (n Int) -> Bool
  :pre (> n 1)
  (not (fold (fn (found Bool) (i Int) -> Bool (or found (== (% n i) 0)))
             false
             (range 2 n))))
""",
    },
    {
        "id": "contains",
        "prompt": "写一个函数 contains(xs, v),判断整数数组是否包含值 v。",
        "cases": [(([1, 2, 3], 2), True), (([1, 2, 3], 5), False), (([], 0), False)],
        "aether_ref": """
(fn contains (xs (Vec Int)) (v Int) -> Bool
  (fold (fn (found Bool) (x Int) -> Bool (or found (== x v))) false xs))
""",
    },
    {
        "id": "count",
        "prompt": "写一个函数 count(xs, v),统计整数数组中值 v 出现的次数。",
        "cases": [(([1, 2, 1], 1), 2), (([], 1), 0), (([5], 5), 1)],
        "aether_ref": """
(fn count (xs (Vec Int)) (v Int) -> Int
  (fold (fn (acc Int) (x Int) -> Int (if (== x v) (+ acc 1) acc)) 0 xs))
""",
    },
    {
        "id": "dot",
        "prompt": "写一个函数 dot(a, b),返回两个等长整数数组的点积。",
        "cases": [(([1, 2, 3], [4, 5, 6]), 32), (([], []), 0)],
        "aether_ref": """
(fn dot (a (Vec Int)) (b (Vec Int)) -> Int
  (fold (fn (acc Int) (i Int) -> Int (+ acc (* (get a i) (get b i)))) 0 (range 0 (len a))))
""",
    },
    {
        "id": "sorted-insert",
        "prompt": "写一个函数 insert(xs, v),把 v 插入已升序整数数组的合适位置,返回新升序数组。",
        "cases": [(([1, 3, 5], 4), [1, 3, 4, 5]), (([], 2), [2]), (([1, 2], 0), [0, 1, 2])],
        "aether_ref": """
(fn insert (xs (Vec Int)) (v Int) -> (Vec Int)
  (if (empty? xs)
      (vec v)
      (if (<= v (head xs))
          (concat (vec v) xs)
          (concat (vec (head xs)) (insert (tail xs) v)))))
""",
    },
    {
        "id": "qsort",
        "prompt": "写一个函数 qsort(xs),对整数数组快速排序。必须带 :post (sorted? result) 契约。",
        "cases": [(([5, 3, 1, 4, 2],), [1, 2, 3, 4, 5]), (([],), []), (([7],), [7])],
        "aether_ref": """
(fn qsort (xs (Vec Int)) -> (Vec Int)
  :post (sorted? result)
  (if (empty? xs)
      xs
      (concat (qsort (filter (fn (x Int) -> Bool (< x (head xs))) xs))
              (vec (head xs))
              (qsort (filter (fn (x Int) -> Bool (> x (head xs))) xs)))))
""",
    },
    {
        "id": "binary-search",
        "prompt": "写一个函数 bsearch(xs, v),在升序整数数组中二分查找 v,返回下标;不存在返回 -1。必须带 :pre (sorted? xs) 契约。",
        "cases": [(([1, 3, 5, 7], 5), 2), (([1, 3, 5, 7], 4), -1), (([], 1), -1)],
        "aether_ref": """
(fn bsearch-lo (xs (Vec Int)) (v Int) (lo Int) (hi Int) -> Int
  :pre (sorted? xs)
  (if (>= lo hi)
      -1
      (block
        (let mid Int (/ (+ lo hi) 2))
        (let mv Int (get xs mid))
        (if (== mv v) mid (if (< mv v) (bsearch-lo xs v (+ mid 1) hi) (bsearch-lo xs v lo mid))))))
(fn bsearch (xs (Vec Int)) (v Int) -> Int
  :pre (sorted? xs)
  (bsearch-lo xs v 0 (len xs)))
""",
    },
    {
        "id": "hanoi",
        "prompt": "写一个函数 hanoi(n),返回 n 层汉诺塔的最少移动步数。必须带 :pre (>= n 0) 契约。",
        "cases": [((0,), 0), ((1,), 1), ((3,), 7), ((10,), 1023)],
        "aether_ref": """
(fn hanoi (n Int) -> Int
  :pre (>= n 0)
  (if (== n 0) 0 (+ 1 (* 2 (hanoi (- n 1))))))
""",
    },
    {
        "id": "power",
        "prompt": "写一个函数 power(a, b),返回 a 的 b 次方(b>=0)。必须带 :pre (>= b 0) 契约。",
        "cases": [((2, 10), 1024), ((3, 0), 1), ((5, 3), 125)],
        "aether_ref": """
(fn power (a Int) (b Int) -> Int
  :pre (>= b 0)
  (if (== b 0) 1 (* a (power a (- b 1)))))
""",
    },
    {
        "id": "digit-sum",
        "prompt": "写一个函数 digit-sum(n),返回非负整数 n 的十进制各位数字之和。必须带 :pre (>= n 0) 契约。",
        "cases": [((0,), 0), ((123,), 6), ((909,), 18)],
        "aether_ref": """
(fn digit-sum (n Int) -> Int
  :pre (>= n 0)
  (if (< n 10) n (+ (% n 10) (digit-sum (/ n 10)))))
""",
    },
    {
        "id": "celsius",
        "prompt": "写一个函数 celsius-to-f(c),把摄氏温度转为华氏: f = c*9/5+32,返回浮点数。",
        "cases": [((0.0,), 32.0), ((100.0,), 212.0), ((-40.0,), -40.0)],
        "aether_ref": """
(fn celsius-to-f (c Float) -> Float
  (+ 32.0 (/ (* c 9.0) 5.0)))
""",
    },
    {
        "id": "triangle",
        "prompt": "写一个函数 is-triangle(a, b, c),判断三边是否能构成三角形(任意两边之和大于第三边)。",
        "cases": [((3, 4, 5), True), ((1, 1, 3), False), ((2, 2, 2), True)],
        "aether_ref": """
(fn is-triangle (a Int) (b Int) (c Int) -> Bool
  (and (> (+ a b) c) (> (+ b c) a) (> (+ c a) b)))
""",
    },
]

def py_ref(task_id: str) -> str:
    """Python 参考实现(用于文档对照,不参与生成)。"""
    refs = {
        "fib": "def fib(n):\n    return n if n < 2 else fib(n - 1) + fib(n - 2)",
        "fact": "def fact(n):\n    return 1 if n <= 1 else n * fact(n - 1)",
        "gcd": "def gcd(a, b):\n    return a if b == 0 else gcd(b, a % b)",
        "sum": "def sum(xs):\n    return 0 if not xs else xs[0] + sum(xs[1:])",
        "max-elem": "def max_elem(xs):\n    return max(xs)",
        "filter-even": "def filter_even(xs):\n    return [x for x in xs if x % 2 == 0]",
        "map-square": "def map_square(xs):\n    return [x * x for x in xs]",
        "reverse": "def reverse(xs):\n    return xs[::-1]",
        "is-prime": "def is_prime(n):\n    return all(n % i for i in range(2, n))",
        "contains": "def contains(xs, v):\n    return v in xs",
        "count": "def count(xs, v):\n    return xs.count(v)",
        "dot": "def dot(a, b):\n    return sum(x * y for x, y in zip(a, b))",
        "sorted-insert": "def insert(xs, v):\n    i = 0\n    while i < len(xs) and xs[i] < v: i += 1\n    return xs[:i] + [v] + xs[i:]",
        "qsort": "def qsort(xs):\n    return xs if len(xs) <= 1 else qsort([x for x in xs[1:] if x < xs[0]]) + [xs[0]] + qsort([x for x in xs[1:] if x >= xs[0]])",
        "binary-search": "def bsearch(xs, v, lo=0, hi=None):\n    if hi is None: hi = len(xs)\n    if lo >= hi: return -1\n    mid = (lo + hi) // 2\n    if xs[mid] == v: return mid\n    return bsearch(xs, v, mid + 1, hi) if xs[mid] < v else bsearch(xs, v, lo, mid)",
        "hanoi": "def hanoi(n):\n    return 0 if n == 0 else 1 + 2 * hanoi(n - 1)",
        "power": "def power(a, b):\n    return 1 if b == 0 else a * power(a, b - 1)",
        "digit-sum": "def digit_sum(n):\n    return n if n < 10 else n % 10 + digit_sum(n // 10)",
        "celsius": "def celsius_to_f(c):\n    return c * 9.0 / 5.0 + 32.0",
        "triangle": "def is_triangle(a, b, c):\n    return a + b > c and b + c > a and c + a > b",
    }
    return refs[task_id]
