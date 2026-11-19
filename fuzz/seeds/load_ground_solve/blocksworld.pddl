; Blocksworld domain — classic STRIPS planning benchmark
; Source: AIPS 1998 planning competition
(define (domain blocksworld)
    (:requirements :strips :typing)
    (:types block)
    (:predicates (on ?x ?y - block)
                 (ontable ?x - block)
                 (clear ?x - block)
                 (handempty)
                 (holding ?x - block))

    (:action pick-up
        :parameters (?x - block)
        :precondition (and (clear ?x) (ontable ?x) (handempty))
        :effect (and (not (ontable ?x))
                     (not (clear ?x))
                     (not (handempty))
                     (holding ?x)))

    (:action put-down
        :parameters (?x - block)
        :precondition (holding ?x)
        :effect (and (not (holding ?x))
                     (clear ?x)
                     (handempty)
                     (ontable ?x)))

    (:action stack
        :parameters (?x ?y - block)
        :precondition (and (holding ?x) (clear ?y))
        :effect (and (not (holding ?x))
                     (not (clear ?y))
                     (clear ?x)
                     (handempty)
                     (on ?x ?y)))

    (:action unstack
        :parameters (?x ?y - block)
        :precondition (and (on ?x ?y) (clear ?x) (handempty))
        :effect (and (holding ?x)
                     (clear ?y)
                     (not (clear ?x))
                     (not (handempty))
                     (not (on ?x ?y))))
)

(define (problem blocksworld-4)
    (:domain blocksworld)
    (:objects a b c d - block)
    (:init (ontable d) (ontable b) (ontable c) (on a c)
           (clear a) (clear b) (clear d) (handempty))
    (:goal (and (on a b) (on b c) (on c d)))
)
