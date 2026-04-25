; Towers of Hanoi domain — move 3 discs across 3 pegs
; Variant using :derived-predicates instead of explicit can-place facts
; Optimal plan length: 7
(define (domain hanoi-derived)
  (:requirements :strips :typing :derived-predicates)

  (:types
    location
    disc peg - location
  )

  (:predicates
    (on ?d - disc ?x - location)
    (clear ?x - location)
    (smaller ?d1 - disc ?d2 - disc)
    (is-peg ?x - peg)
  )

  (:derived (can-place ?d - disc ?x - location)
    (or
      (is-peg ?x)
      (smaller ?d ?x)
    )
  )

  (:action move-disc
    :parameters (?d - disc ?from ?to - location)
    :precondition
      (and
        (on ?d ?from)
        (clear ?d)
        (clear ?to)
        (can-place ?d ?to)
      )
    :effect
      (and
        (on ?d ?to)
        (clear ?from)
        (not (on ?d ?from))
        (not (clear ?to))
      )
  )
)

(define (problem hanoi-derived-3)
  (:domain hanoi-derived)

  (:objects
    d1 d2 d3 - disc
    peg1 peg2 peg3 - peg
  )

  (:init
    (on d1 d2)
    (on d2 d3)
    (on d3 peg1)

    (clear d1)
    (clear peg2)
    (clear peg3)

    (smaller d1 d2)
    (smaller d1 d3)
    (smaller d2 d3)

    (is-peg peg1)
    (is-peg peg2)
    (is-peg peg3)
  )

  (:goal
    (and
      (on d1 d2)
      (on d2 d3)
      (on d3 peg3)
    )
  )
)
