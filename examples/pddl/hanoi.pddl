; Towers of Hanoi domain — move 3 discs across 3 pegs
; Source: AIMA textbook / classical planning benchmark
; Optimal plan length: 7
(define (domain hanoi)
  (:requirements :strips :typing)

  (:types
    location
    disc peg - location
  )

  (:predicates
    (on ?d - disc ?x - location)
    (clear ?x - location)
    (can-place ?d - disc ?x - location)
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

(define (problem hanoi-3)
  (:domain hanoi)

  (:objects
    d1 d2 d3 - disc
    peg1 peg2 peg3 - peg
  )

  (:init
    ;; d1 smallest, d3 largest
    (on d1 d2)
    (on d2 d3)
    (on d3 peg1)

    (clear d1)
    (clear peg2)
    (clear peg3)

    ;; pegs can hold any disc
    (can-place d1 peg1)
    (can-place d1 peg2)
    (can-place d1 peg3)

    (can-place d2 peg1)
    (can-place d2 peg2)
    (can-place d2 peg3)

    (can-place d3 peg1)
    (can-place d3 peg2)
    (can-place d3 peg3)

    ;; smaller discs can go on larger discs
    (can-place d1 d2)
    (can-place d1 d3)
    (can-place d2 d3)
  )

  (:goal
    (and
      (on d1 d2)
      (on d2 d3)
      (on d3 peg3)
    )
  )
)