; Grid domain — navigation with keys and locked doors
; Source: IPC 1998 Grid domain, simplified
; Requires :negative-preconditions for locked cell checks
(define (domain grid)
    (:requirements :strips :typing :negative-preconditions)
    (:types cell key)
    (:predicates (at-robot ?c - cell)
                 (connected ?c1 ?c2 - cell)
                 (has-key ?k - key)
                 (locked ?c - cell)
                 (at-key ?k - key ?c - cell))

    (:action move
        :parameters (?from ?to - cell)
        :precondition (and (at-robot ?from) (connected ?from ?to) (not (locked ?to)))
        :effect (and (at-robot ?to) (not (at-robot ?from))))

    (:action pickup-key
        :parameters (?k - key ?c - cell)
        :precondition (and (at-robot ?c) (at-key ?k ?c))
        :effect (and (has-key ?k) (not (at-key ?k ?c))))

    (:action unlock
        :parameters (?k - key ?c - cell)
        :precondition (has-key ?k)
        :effect (not (locked ?c)))
)

(define (problem grid-small)
    (:domain grid)
    (:objects c1 c2 c3 c4 c5 c6 - cell
              key1 - key)
    (:init
        ;; 2x3 grid layout: c1-c2-c3 / c4-c5-c6
        (connected c1 c2) (connected c2 c1)
        (connected c2 c3) (connected c3 c2)
        (connected c4 c5) (connected c5 c4)
        (connected c5 c6) (connected c6 c5)
        (connected c1 c4) (connected c4 c1)
        (connected c2 c5) (connected c5 c2)
        (connected c3 c6) (connected c6 c3)
        (at-robot c1)
        (at-key key1 c3)
        (locked c6))
    (:goal (and (at-robot c6) (has-key key1)))
)
