; Eight Puzzle domain — 3×3 sliding tile puzzle
; Source: sunsided/aiplanning (https://github.com/sunsided/aiplanning)
;
; Uses typed positions and a single "blank-at" fluent plus "tile-at" fluents.
; The move action swaps the blank with an adjacent tile.
(define (domain eight-puzzle)
    (:requirements :strips :typing)
    (:types position tile)
    (:predicates (blank-at ?p - position)
                 (tile-at ?t - tile ?p - position)
                 (adjacent ?p1 ?p2 - position))

    (:action move-tile
        :parameters (?from ?to - position ?t - tile)
        :precondition (and (tile-at ?t ?from) (blank-at ?to) (adjacent ?from ?to))
        :effect (and (tile-at ?t ?to) (blank-at ?from)
                     (not (tile-at ?t ?from)) (not (blank-at ?to))))
)

(define (problem eight-puzzle-scrambled)
    (:domain eight-puzzle)
    (:objects p1 p2 p3 p4 p5 p6 p7 p8 p9 - position
              t1 t2 t3 t4 t5 t6 t7 t8 - tile)
    (:init
        ;; Adjacency (grid layout: p1 p2 p3 / p4 p5 p6 / p7 p8 p9)
        (adjacent p1 p2) (adjacent p2 p1)
        (adjacent p2 p3) (adjacent p3 p2)
        (adjacent p1 p4) (adjacent p4 p1)
        (adjacent p2 p5) (adjacent p5 p2)
        (adjacent p3 p6) (adjacent p6 p3)
        (adjacent p4 p5) (adjacent p5 p4)
        (adjacent p4 p7) (adjacent p7 p4)
        (adjacent p5 p6) (adjacent p6 p5)
        (adjacent p5 p8) (adjacent p8 p5)
        (adjacent p6 p9) (adjacent p9 p6)
        (adjacent p7 p8) (adjacent p8 p7)
        (adjacent p8 p9) (adjacent p9 p8)
        ;; Goal state: t1 t2 t3 / t4 t5 t6 / t7 t8 blank, blank at p9
        ;; Scrambled: apply 5 random moves from goal to get a small-scramble instance
        ;; Goal configuration scrambled by: swap(p9,p6), swap(p6,p3), swap(p3,p2), swap(p2,p5), swap(p5,p8)
        ;; Starting: t1 t5 t2 / t4 blank t6 / t7 t8 t3, blank at p5
        (tile-at t1 p1) (tile-at t5 p2) (tile-at t2 p3)
        (tile-at t4 p4) (tile-at t6 p6)
        (tile-at t7 p7) (tile-at t8 p8) (tile-at t3 p9)
        (blank-at p5))
    (:goal (and (tile-at t1 p1) (tile-at t2 p2) (tile-at t3 p3)
                (tile-at t4 p4) (tile-at t5 p5) (tile-at t6 p6)
                (tile-at t7 p7) (tile-at t8 p8) (blank-at p9)))
)

(define (problem eight-puzzle-easy)
    (:domain eight-puzzle)
    (:objects p1 p2 p3 p4 p5 p6 p7 p8 p9 - position
              t1 t2 t3 t4 t5 t6 t7 t8 - tile)
    (:init
        (adjacent p1 p2) (adjacent p2 p1)
        (adjacent p2 p3) (adjacent p3 p2)
        (adjacent p1 p4) (adjacent p4 p1)
        (adjacent p2 p5) (adjacent p5 p2)
        (adjacent p3 p6) (adjacent p6 p3)
        (adjacent p4 p5) (adjacent p5 p4)
        (adjacent p4 p7) (adjacent p7 p4)
        (adjacent p5 p6) (adjacent p6 p5)
        (adjacent p5 p8) (adjacent p8 p5)
        (adjacent p6 p9) (adjacent p9 p6)
        (adjacent p7 p8) (adjacent p8 p7)
        (adjacent p8 p9) (adjacent p9 p8)
        ;; 2 moves from goal: slide t6 down, then t3 right
        (tile-at t1 p1) (tile-at t2 p2) (tile-at t3 p6)
        (tile-at t4 p4) (tile-at t5 p5) (tile-at t6 p9)
        (tile-at t7 p7) (tile-at t8 p8)
        (blank-at p3))
    (:goal (and (tile-at t1 p1) (tile-at t2 p2) (tile-at t3 p3)
                (tile-at t4 p4) (tile-at t5 p5) (tile-at t6 p6)
                (tile-at t7 p7) (tile-at t8 p8) (blank-at p9)))
)
