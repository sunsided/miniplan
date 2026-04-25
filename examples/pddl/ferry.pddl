; Ferry domain — single-car ferry shuttling cars between shores
; Source: IPC / AIMA textbook, simplified logistics variant
(define (domain ferry)
    (:requirements :strips :typing)
    (:types car shore)
    (:predicates (at ?c - car ?s - shore)
                 (at-ferry ?s - shore)
                 (on-board ?c - car)
                 (empty-ferry))

    (:action sail
        :parameters (?from ?to - shore)
        :precondition (at-ferry ?from)
        :effect (and (at-ferry ?to) (not (at-ferry ?from))))

    (:action embark
        :parameters (?c - car ?s - shore)
        :precondition (and (at ?c ?s) (at-ferry ?s) (empty-ferry))
        :effect (and (on-board ?c) (not (at ?c ?s)) (not (empty-ferry))))

    (:action disembark
        :parameters (?c - car ?s - shore)
        :precondition (and (on-board ?c) (at-ferry ?s))
        :effect (and (at ?c ?s) (empty-ferry) (not (on-board ?c))))
)

(define (problem ferry-2)
    (:domain ferry)
    (:objects car1 car2 - car
              shore-a shore-b - shore)
    (:init (at car1 shore-a) (at car2 shore-a)
           (at-ferry shore-a) (empty-ferry))
    (:goal (and (at car1 shore-b) (at car2 shore-b)))
)
