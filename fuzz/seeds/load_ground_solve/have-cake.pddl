; Have Cake domain — eat a cake and bake another one
; Source: AIND-Planning (https://github.com/sunsided/AIND-Planning)
(define (domain have-cake)
    (:requirements :strips :negative-preconditions)
    (:predicates (have-cake) (eaten-cake))

    (:action eat
        :parameters ()
        :precondition (have-cake)
        :effect (and (eaten-cake) (not (have-cake))))

    (:action bake
        :parameters ()
        :precondition (not (have-cake))
        :effect (and (have-cake) (not (eaten-cake))))
)

; You won't.
(define (problem have-cake-problem)
    (:domain have-cake)
    (:init (have-cake))
    (:goal (and (have-cake) (eaten-cake)))
)
