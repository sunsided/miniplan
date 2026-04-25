(define (domain air-cargo)
    (:requirements :strips :typing)
    (:types cargo plane airport)
    (:predicates (at ?obj - (either cargo plane) ?loc - airport)
                 (in ?c - cargo ?p - plane))

    (:action load
        :parameters (?c - cargo ?p - plane ?a - airport)
        :precondition (and (at ?c ?a) (at ?p ?a))
        :effect (and (in ?c ?p) (not (at ?c ?a))))

    (:action unload
        :parameters (?c - cargo ?p - plane ?a - airport)
        :precondition (and (in ?c ?p) (at ?p ?a))
        :effect (and (at ?c ?a) (not (in ?c ?p))))

    (:action fly
        :parameters (?p - plane ?from ?to - airport)
        :precondition (at ?p ?from)
        :effect (and (at ?p ?to) (not (at ?p ?from))))
)

(define (problem air-cargo-p3)
    (:domain air-cargo)
    (:objects c1 c2 c3 c4 - cargo
              p1 p2      - plane
              jfk sfo atl ord - airport)
    (:init (at c1 sfo) (at c2 jfk) (at c3 atl) (at c4 ord)
           (at p1 sfo) (at p2 jfk))
    (:goal (and (at c1 jfk) (at c3 jfk)
                (at c2 sfo) (at c4 sfo)))
)
