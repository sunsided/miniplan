(define (domain zombies-survivors)
  (:requirements :typing :negative-preconditions :numeric-fluents)

  (:types load)

  (:predicates
    (boat-left)
  )

  (:functions
    (survivors-left)
    (zombies-left)
    (total-survivors)
    (total-zombies)
    (survivor-load ?l - load)
    (zombie-load ?l - load)
  )

  (:action cross-left-right
    :parameters (?l - load)
    :precondition
      (and
        (boat-left)

        (>= (survivors-left) (survivor-load ?l))
        (>= (zombies-left) (zombie-load ?l))

        (>= (+ (survivor-load ?l) (zombie-load ?l)) 1)
        (<= (+ (survivor-load ?l) (zombie-load ?l)) 2)

        ;; left bank after departure is safe
        (or
          (= (- (survivors-left) (survivor-load ?l)) 0)
          (>=
            (- (survivors-left) (survivor-load ?l))
            (- (zombies-left) (zombie-load ?l))))

        ;; right bank after arrival is safe
        (or
          (= (+ (- (total-survivors) (survivors-left))
                (survivor-load ?l)) 0)
          (>=
            (+ (- (total-survivors) (survivors-left))
               (survivor-load ?l))
            (+ (- (total-zombies) (zombies-left))
               (zombie-load ?l)))))
    :effect
      (and
        (decrease (survivors-left) (survivor-load ?l))
        (decrease (zombies-left) (zombie-load ?l))
        (not (boat-left)))
  )

  (:action cross-right-left
    :parameters (?l - load)
    :precondition
      (and
        (not (boat-left))

        (>= (- (total-survivors) (survivors-left)) (survivor-load ?l))
        (>= (- (total-zombies) (zombies-left)) (zombie-load ?l))

        (>= (+ (survivor-load ?l) (zombie-load ?l)) 1)
        (<= (+ (survivor-load ?l) (zombie-load ?l)) 2)

        ;; left bank after return is safe
        (or
          (= (+ (survivors-left) (survivor-load ?l)) 0)
          (>=
            (+ (survivors-left) (survivor-load ?l))
            (+ (zombies-left) (zombie-load ?l))))

        ;; right bank after departure is safe
        (or
          (= (- (- (total-survivors) (survivors-left))
                (survivor-load ?l)) 0)
          (>=
            (- (- (total-survivors) (survivors-left))
               (survivor-load ?l))
            (- (- (total-zombies) (zombies-left))
               (zombie-load ?l)))))
    :effect
      (and
        (increase (survivors-left) (survivor-load ?l))
        (increase (zombies-left) (zombie-load ?l))
        (boat-left))
  )
)

(define (problem zs-3-3)
  (:domain zombies-survivors)

  (:objects
    one-survivor
    one-zombie
    two-survivors
    two-zombies
    one-survivor-one-zombie - load
  )

  (:init
    (boat-left)

    (= (survivors-left) 3)
    (= (zombies-left) 3)
    (= (total-survivors) 3)
    (= (total-zombies) 3)

    (= (survivor-load one-survivor) 1)
    (= (zombie-load one-survivor) 0)

    (= (survivor-load one-zombie) 0)
    (= (zombie-load one-zombie) 1)

    (= (survivor-load two-survivors) 2)
    (= (zombie-load two-survivors) 0)

    (= (survivor-load two-zombies) 0)
    (= (zombie-load two-zombies) 2)

    (= (survivor-load one-survivor-one-zombie) 1)
    (= (zombie-load one-survivor-one-zombie) 1)
  )

  (:goal
    (and
      (= (survivors-left) 0)
      (= (zombies-left) 0)))
)