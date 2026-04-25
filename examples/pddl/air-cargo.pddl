; Air Cargo domain — transport cargo between airports using planes
; handles (either cargo plane) typing
;
; Three canonical AIND air-cargo scenarios:
;   air-cargo-p1 — 2 cargos / 2 planes / 2 airports; optimal plan length 6
;   air-cargo-p2 — 3 cargos / 3 planes / 3 airports; optimal plan length 9
;   air-cargo-p3 — 4 cargos / 2 planes / 4 airports; optimal plan length 12
;
; Based on https://github.com/sunsided/AIND-Planning/blob/master/tex/heuristic_analysis.tex
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

(define (problem air-cargo-p1)
    (:domain air-cargo)
    (:objects c1 c2 - cargo
              p1 p2 - plane
              sfo jfk - airport)
    (:init (at c1 sfo) (at c2 jfk)
           (at p1 sfo) (at p2 jfk))
    (:goal (and (at c1 jfk) (at c2 sfo)))
)

(define (problem air-cargo-p2)
    (:domain air-cargo)
    (:objects c1 c2 c3 - cargo
              p1 p2 p3 - plane
              sfo jfk atl - airport)
    (:init (at c1 sfo) (at c2 jfk) (at c3 atl)
           (at p1 sfo) (at p2 jfk) (at p3 atl))
    (:goal (and (at c1 jfk) (at c2 sfo) (at c3 sfo)))
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
